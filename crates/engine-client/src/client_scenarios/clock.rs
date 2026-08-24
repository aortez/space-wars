use std::cell::Cell;
use std::time::Duration;

use chrono::{Local, Timelike};
use engine_common::{Action, RenderFrame, Scenario, Settings, StepResult, TickModel};
use scenario_clock::{ClockAction, ClockConfig, ClockReading, ClockScenario, ClockState};

use super::{
    ClientScenario, RenderBackend, ScenarioAsset, ScenarioCapabilities, ScenarioCreateError,
    ScenarioRegistration, ScenarioStartMode,
};
use crate::input::ClientInput;
use crate::render::{FrameLayout, Viewport};

pub(super) const REGISTRATION: ScenarioRegistration = ScenarioRegistration {
    id: "clock",
    launcher_visible: true,
    capabilities: ScenarioCapabilities {
        benchmark: false,
        pointer_input: false,
        player_zoom: false,
        game_over: false,
        native_video: false,
        captures_gamepad_start: false,
        captures_gamepad_select: false,
    },
    controls_help: "Clock follows the device's local time and needs no in-scenario controls. Use the host pause menu to restart it or return to the launcher.",
    create,
};

pub(crate) struct ClockClientScenario {
    pub(crate) state: ClockState,
    last_emitted_reading: Cell<Option<ClockReading>>,
}

impl ClockClientScenario {
    fn actions_for_reading(&self, reading: ClockReading) -> Vec<Action> {
        if self.last_emitted_reading.replace(Some(reading)) == Some(reading) {
            Vec::new()
        } else {
            vec![ClockAction::set_reading(reading)]
        }
    }
}

fn create(
    seed: u64,
    settings: &Settings,
    viewport: Viewport,
    _mode: ScenarioStartMode,
    _asset: &ScenarioAsset,
) -> Result<Box<dyn ClientScenario>, ScenarioCreateError> {
    let mut state = ClockScenario::init(
        ClockConfig {
            aspect_ratio: viewport.aspect_ratio(),
            time_format: settings.clock.time_format,
        },
        seed,
    );
    let reading = local_clock_reading();
    ClockScenario::step(
        &mut state,
        &[ClockAction::set_reading(reading)],
        Duration::ZERO,
    );
    Ok(Box::new(ClockClientScenario {
        state,
        last_emitted_reading: Cell::new(Some(reading)),
    }))
}

impl ClientScenario for ClockClientScenario {
    fn registration(&self) -> &'static ScenarioRegistration {
        &REGISTRATION
    }

    fn tick_model(&self) -> TickModel {
        ClockScenario::tick_model()
    }

    fn step(&mut self, actions: &[Action], dt: Duration) -> StepResult {
        ClockScenario::step(&mut self.state, actions, dt)
    }

    fn map_input(&self, _input: &mut ClientInput, _benchmark_active: bool) -> Vec<Action> {
        self.actions_for_reading(local_clock_reading())
    }

    fn render_frames(&self, _renderer: RenderBackend, _viewport: Viewport) -> Vec<RenderFrame> {
        vec![ClockScenario::render_frame(&self.state)]
    }

    fn frame_layout(&self) -> FrameLayout {
        FrameLayout::EqualHorizontal
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        self.state.set_aspect_ratio(viewport.aspect_ratio());
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

fn local_clock_reading() -> ClockReading {
    let now = Local::now();
    ClockReading::new(now.hour() as u8, now.minute() as u8, now.second() as u8)
        .expect("chrono always returns a valid local clock reading")
}

#[cfg(test)]
mod tests {
    use engine_common::ClockTimeFormat;

    use super::*;

    #[test]
    fn factory_applies_a_valid_initial_local_reading_and_settings() {
        let mut settings = Settings::default();
        settings.clock.time_format = ClockTimeFormat::TwelveHour;
        let scenario = create(
            17,
            &settings,
            Viewport::new(800.0, 480.0),
            ScenarioStartMode::Normal,
            &ScenarioAsset::None,
        )
        .unwrap();
        let scenario = scenario
            .as_any()
            .downcast_ref::<ClockClientScenario>()
            .unwrap();

        assert!(scenario.state.reading().is_some());
        assert_eq!(scenario.state.time_format(), ClockTimeFormat::TwelveHour);
        assert_eq!(scenario.state.aspect_ratio(), 800.0 / 480.0);
    }

    #[test]
    fn adapter_suppresses_duplicate_readings() {
        let scenario = create(
            0,
            &Settings::default(),
            Viewport::new(800.0, 480.0),
            ScenarioStartMode::Normal,
            &ScenarioAsset::None,
        )
        .unwrap();
        let scenario = scenario
            .as_any()
            .downcast_ref::<ClockClientScenario>()
            .unwrap();
        let first = ClockReading::new(10, 20, 30).unwrap();
        let second = ClockReading::new(10, 20, 31).unwrap();
        scenario.last_emitted_reading.set(Some(first));
        assert!(scenario.actions_for_reading(first).is_empty());

        let actions = scenario.actions_for_reading(second);
        assert_eq!(actions.len(), 1);
        assert_eq!(
            ClockAction::decode(&actions[0]),
            Some(ClockAction::SetReading(second))
        );
    }
}
