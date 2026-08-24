//! Deterministic low-resolution clock scenario.

mod digits;
mod render;

use std::time::Duration;

pub use digits::{
    DIGIT_SLOT_COUNT, DisplaySnapshot, GridCell, SegmentId, SegmentKind, SegmentRepresentation,
    SegmentState,
};

use engine_common::{
    Action, ClockTimeFormat, Observation, RenderFrame, Scenario, StepResult, TickModel,
};

pub const CLOCK_ACTION_VERSION: u16 = 1;
pub const CLOCK_ACTION_SET_READING: u32 = 1;
pub const CLOCK_OBSERVATION_VERSION: u16 = 1;

const DEFAULT_ASPECT_RATIO: f32 = 800.0 / 480.0;
const MIN_ASPECT_RATIO: f32 = 0.25;
const MAX_ASPECT_RATIO: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockReading {
    hour: u8,
    minute: u8,
    second: u8,
}

impl ClockReading {
    pub const fn new(hour: u8, minute: u8, second: u8) -> Option<Self> {
        if hour < 24 && minute < 60 && second < 60 {
            Some(Self {
                hour,
                minute,
                second,
            })
        } else {
            None
        }
    }

    pub const fn hour(self) -> u8 {
        self.hour
    }

    pub const fn minute(self) -> u8 {
        self.minute
    }

    pub const fn second(self) -> u8 {
        self.second
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockAction {
    SetReading(ClockReading),
}

impl ClockAction {
    pub fn set_reading(reading: ClockReading) -> Action {
        let mut payload = Vec::with_capacity(5);
        payload.extend_from_slice(&CLOCK_ACTION_VERSION.to_le_bytes());
        payload.extend_from_slice(&[reading.hour, reading.minute, reading.second]);
        Action::scenario(CLOCK_ACTION_SET_READING, payload)
    }

    pub fn decode(action: &Action) -> Option<Self> {
        let Action::Scenario { kind, payload } = action else {
            return None;
        };
        if *kind != CLOCK_ACTION_SET_READING || payload.len() != 5 {
            return None;
        }
        let version = u16::from_le_bytes(payload[0..2].try_into().ok()?);
        if version != CLOCK_ACTION_VERSION {
            return None;
        }
        ClockReading::new(payload[2], payload[3], payload[4]).map(Self::SetReading)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockConfig {
    pub aspect_ratio: f32,
    pub time_format: ClockTimeFormat,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            aspect_ratio: DEFAULT_ASPECT_RATIO,
            time_format: ClockTimeFormat::TwentyFourHour,
        }
    }
}

impl ClockConfig {
    fn normalized(self) -> Self {
        Self {
            aspect_ratio: normalize_aspect_ratio(self.aspect_ratio),
            time_format: self.time_format,
        }
    }
}

#[derive(Debug)]
pub struct ClockState {
    config: ClockConfig,
    reading: Option<ClockReading>,
    display: DisplaySnapshot,
    segments: Vec<SegmentState>,
}

impl ClockState {
    pub fn reading(&self) -> Option<ClockReading> {
        self.reading
    }

    pub fn display(&self) -> DisplaySnapshot {
        self.display
    }

    pub fn segments(&self) -> &[SegmentState] {
        &self.segments
    }

    pub fn time_format(&self) -> ClockTimeFormat {
        self.config.time_format
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.config.aspect_ratio
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.config.aspect_ratio = normalize_aspect_ratio(aspect_ratio);
    }

    fn apply_reading(&mut self, reading: ClockReading) {
        self.reading = Some(reading);
        self.display = digits::snapshot(reading, self.config.time_format);
        digits::apply_snapshot(&mut self.segments, self.display);
    }
}

pub struct ClockScenario;

impl Scenario for ClockScenario {
    type State = ClockState;
    type Config = ClockConfig;

    fn init(config: Self::Config, _seed: u64) -> Self::State {
        ClockState {
            config: config.normalized(),
            reading: None,
            display: DisplaySnapshot::unsynchronized(),
            segments: digits::create_segments(),
        }
    }

    fn step(state: &mut Self::State, actions: &[Action], _dt: Duration) -> StepResult {
        if let Some(ClockAction::SetReading(reading)) =
            actions.iter().filter_map(ClockAction::decode).next_back()
        {
            state.apply_reading(reading);
        }
        StepResult::default()
    }

    fn observe(state: &Self::State) -> Observation {
        let mut payload = Vec::with_capacity(8);
        payload.extend_from_slice(&CLOCK_OBSERVATION_VERSION.to_le_bytes());
        payload.push(u8::from(state.reading.is_some()));
        if let Some(reading) = state.reading {
            payload.extend_from_slice(&[reading.hour, reading.minute, reading.second]);
        } else {
            payload.extend_from_slice(&[u8::MAX; 3]);
        }
        payload.push(match state.config.time_format {
            ClockTimeFormat::TwelveHour => 12,
            ClockTimeFormat::TwentyFourHour => 24,
        });
        Observation { payload }
    }

    fn render_frame(state: &Self::State) -> RenderFrame {
        render::render_frame(state)
    }

    fn tick_model() -> TickModel {
        TickModel::FixedTimestep { hz: 60 }
    }
}

fn normalize_aspect_ratio(aspect_ratio: f32) -> f32 {
    if aspect_ratio.is_finite() && aspect_ratio > 0.0 {
        aspect_ratio.clamp(MIN_ASPECT_RATIO, MAX_ASPECT_RATIO)
    } else {
        DEFAULT_ASPECT_RATIO
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(hour: u8, minute: u8, second: u8) -> ClockReading {
        ClockReading::new(hour, minute, second).unwrap()
    }

    #[test]
    fn reading_action_round_trips_and_rejects_invalid_payloads() {
        let action = ClockAction::set_reading(reading(19, 42, 7));
        assert_eq!(
            ClockAction::decode(&action),
            Some(ClockAction::SetReading(reading(19, 42, 7)))
        );

        assert_eq!(
            ClockAction::decode(&Action::scenario(
                CLOCK_ACTION_SET_READING,
                vec![1, 0, 24, 0, 0],
            )),
            None
        );
        assert_eq!(
            ClockAction::decode(&Action::scenario(
                CLOCK_ACTION_SET_READING,
                vec![2, 0, 19, 42, 7],
            )),
            None
        );
    }

    #[test]
    fn latest_valid_reading_wins_without_advancing_wall_time() {
        let mut state = ClockScenario::init(ClockConfig::default(), 9);
        let original_ids = state
            .segments()
            .iter()
            .map(|segment| segment.id)
            .collect::<Vec<_>>();
        ClockScenario::step(
            &mut state,
            &[
                ClockAction::set_reading(reading(10, 11, 12)),
                Action::scenario(CLOCK_ACTION_SET_READING, vec![1, 0, 99, 0, 0]),
                ClockAction::set_reading(reading(10, 12, 13)),
            ],
            Duration::from_secs(30),
        );

        assert_eq!(state.reading(), Some(reading(10, 12, 13)));
        assert_eq!(state.display().digits, [Some(1), Some(0), Some(1), Some(2)]);
        assert!(!state.display().colon_lit);
        assert_eq!(
            state
                .segments()
                .iter()
                .map(|segment| segment.id)
                .collect::<Vec<_>>(),
            original_ids
        );

        ClockScenario::step(&mut state, &[], Duration::from_secs(60));
        assert_eq!(state.reading(), Some(reading(10, 12, 13)));
    }

    #[test]
    fn observation_reports_sync_state_reading_and_format() {
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: 16.0 / 9.0,
                time_format: ClockTimeFormat::TwelveHour,
            },
            0,
        );
        assert_eq!(
            ClockScenario::observe(&state).payload,
            vec![1, 0, 0, 255, 255, 255, 12]
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(23, 59, 58))],
            Duration::ZERO,
        );
        assert_eq!(
            ClockScenario::observe(&state).payload,
            vec![1, 0, 1, 23, 59, 58, 12]
        );
    }

    #[test]
    fn aspect_ratio_is_normalized_on_init_and_resize() {
        let mut state = ClockScenario::init(
            ClockConfig {
                aspect_ratio: f32::NAN,
                ..ClockConfig::default()
            },
            0,
        );
        assert_eq!(state.aspect_ratio(), DEFAULT_ASPECT_RATIO);
        state.set_aspect_ratio(100.0);
        assert_eq!(state.aspect_ratio(), MAX_ASPECT_RATIO);
        state.set_aspect_ratio(0.0);
        assert_eq!(state.aspect_ratio(), DEFAULT_ASPECT_RATIO);
    }
}
