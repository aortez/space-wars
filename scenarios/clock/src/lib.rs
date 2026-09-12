//! Deterministic low-resolution clock scenario.

#[cfg(test)]
mod digit_slide_tests;
mod digits;
#[cfg(test)]
mod event_tests;
mod events;
mod layout;
#[cfg(test)]
mod live_tests;
mod physics;
mod presentation;
mod render;

use std::time::Duration;

pub use digits::{
    DIGIT_SLOT_COUNT, DisplaySnapshot, GridCell, SegmentId, SegmentKind, SegmentRepresentation,
    SegmentState,
};

use engine_common::{
    Action, ClockEventKind, ClockEventProfile, ClockEvents, ClockMarqueeMessage,
    ClockMarqueePreset, ClockSettings, ClockTimeFormat, Observation, RenderFrame, Scenario,
    StepResult, TickModel,
};
pub use events::digit_slide::DIGIT_SLIDE_TICKS;
pub use events::duck::DUCK_TICKS;
pub use events::marquee::MARQUEE_TICKS;
pub use events::meltdown::{
    DRAINING_TICKS, MAX_MELTDOWN_CELLS, MAX_SPILL_PARCELS, MELTING_TICKS, WATER_COLUMNS,
};
use events::{ActiveEvent, EventContext, EventSchedule};
pub use events::{
    COLOR_CYCLE_TICKS, COOLDOWN_TICKS, DigitPalette, EVENT_CATALOG, EventDefinition, EventEffect,
    EventLifecycle, EventPhase, FALLING_TICKS, FIXED_HZ, REFORMING_TICKS,
};
use layout::Layout;

pub const CLOCK_ACTION_VERSION: u16 = 4;
pub const CLOCK_ACTION_SET_READING: u32 = 1;
pub const CLOCK_ACTION_TRIGGER_EVENT: u32 = 3;
pub const CLOCK_ACTION_CONFIGURE: u32 = 4;
pub const CLOCK_ACTION_PREVIEW_EVENT: u32 = 5;
pub const CLOCK_OBSERVATION_VERSION: u16 = 1;

const DEFAULT_ASPECT_RATIO: f32 = 800.0 / 480.0;
const MIN_ASPECT_RATIO: f32 = 0.25;
const MAX_ASPECT_RATIO: f32 = 4.0;
const MAX_CONFIGURE_BYTES: usize = 6 + engine_common::MAX_CLOCK_MESSAGE_BYTES;

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
    TriggerEvent(ClockEventKind),
    Configure(ClockSettings),
    PreviewEvent(ClockEventKind),
}

impl ClockAction {
    pub fn configure(settings: ClockSettings) -> Action {
        let mut payload = CLOCK_ACTION_VERSION.to_le_bytes().to_vec();
        payload.push(match settings.time_format {
            ClockTimeFormat::TwelveHour => 12,
            ClockTimeFormat::TwentyFourHour => 24,
        });
        payload.push(match settings.event_profile {
            ClockEventProfile::Off => 0,
            ClockEventProfile::Calm => 1,
            ClockEventProfile::Demo => 2,
        });
        payload.push(
            u8::from(settings.events.falling)
                | (u8::from(settings.events.color_cycle) << 1)
                | (u8::from(settings.events.meltdown) << 2)
                | (u8::from(settings.events.duck) << 3)
                | (u8::from(settings.events.marquee) << 4)
                | (u8::from(settings.events.digit_slide) << 5),
        );
        payload.push(settings.marquee_preset as u8);
        payload.extend_from_slice(settings.marquee_message.as_str().as_bytes());
        Action::scenario(CLOCK_ACTION_CONFIGURE, payload)
    }

    pub fn preview_event(kind: ClockEventKind) -> Action {
        let mut payload = CLOCK_ACTION_VERSION.to_le_bytes().to_vec();
        payload.push(kind as u8);
        Action::scenario(CLOCK_ACTION_PREVIEW_EVENT, payload)
    }

    pub fn trigger_event(kind: ClockEventKind) -> Action {
        let mut payload = CLOCK_ACTION_VERSION.to_le_bytes().to_vec();
        payload.push(kind as u8);
        Action::scenario(CLOCK_ACTION_TRIGGER_EVENT, payload)
    }

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
        let version = u16::from_le_bytes(payload.get(0..2)?.try_into().ok()?);
        if version != CLOCK_ACTION_VERSION {
            return None;
        }
        match (*kind, payload.len()) {
            (CLOCK_ACTION_SET_READING, 5) => {
                ClockReading::new(payload[2], payload[3], payload[4]).map(Self::SetReading)
            }
            (CLOCK_ACTION_TRIGGER_EVENT, 3) => ClockEventKind::ALL
                .into_iter()
                .find(|kind| *kind as u8 == payload[2])
                .map(Self::TriggerEvent),
            (CLOCK_ACTION_PREVIEW_EVENT, 3) => ClockEventKind::ALL
                .into_iter()
                .find(|kind| *kind as u8 == payload[2])
                .map(Self::PreviewEvent),
            (CLOCK_ACTION_CONFIGURE, 7..=MAX_CONFIGURE_BYTES) if payload[4] <= 63 => {
                Some(Self::Configure(ClockSettings {
                    time_format: match payload[2] {
                        12 => ClockTimeFormat::TwelveHour,
                        24 => ClockTimeFormat::TwentyFourHour,
                        _ => return None,
                    },
                    event_profile: match payload[3] {
                        0 => ClockEventProfile::Off,
                        1 => ClockEventProfile::Calm,
                        2 => ClockEventProfile::Demo,
                        _ => return None,
                    },
                    events: ClockEvents {
                        falling: payload[4] & 1 != 0,
                        color_cycle: payload[4] & 2 != 0,
                        meltdown: payload[4] & 4 != 0,
                        duck: payload[4] & 8 != 0,
                        marquee: payload[4] & 16 != 0,
                        digit_slide: payload[4] & 32 != 0,
                    },
                    marquee_preset: *ClockMarqueePreset::ALL.get(usize::from(payload[5]))?,
                    marquee_message: std::str::from_utf8(&payload[6..]).ok()?.parse().ok()?,
                }))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockConfig {
    pub aspect_ratio: f32,
    pub duck_debug_overlay: bool,
    /// Development-only collecting-pool preview using the Meltdown lifecycle.
    pub water_lab: bool,
    /// None chooses a seeded personality once per Duck event.
    pub duck_jump_profile: Option<engine_common::ClockDuckJumpProfile>,
    /// None selects a seeded course pattern, independently of personality.
    pub duck_course_pattern: Option<engine_common::ClockDuckCoursePattern>,
    pub time_format: ClockTimeFormat,
    pub event_profile: ClockEventProfile,
    pub events: ClockEvents,
    pub marquee_preset: ClockMarqueePreset,
    pub marquee_message: ClockMarqueeMessage,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            aspect_ratio: DEFAULT_ASPECT_RATIO,
            duck_debug_overlay: false,
            water_lab: false,
            duck_jump_profile: None,
            duck_course_pattern: None,
            time_format: ClockTimeFormat::TwentyFourHour,
            event_profile: ClockEventProfile::default(),
            events: ClockEvents::default(),
            marquee_preset: ClockMarqueePreset::default(),
            marquee_message: ClockMarqueeMessage::default(),
        }
    }
}

impl ClockConfig {
    fn normalized(self) -> Self {
        Self {
            aspect_ratio: normalize_aspect_ratio(self.aspect_ratio),
            duck_debug_overlay: self.duck_debug_overlay,
            water_lab: self.water_lab,
            duck_jump_profile: self.duck_jump_profile,
            duck_course_pattern: self.duck_course_pattern,
            time_format: self.time_format,
            event_profile: self.event_profile,
            events: self.events,
            marquee_preset: self.marquee_preset,
            marquee_message: self.marquee_message,
        }
    }
}

pub struct ClockState {
    config: ClockConfig,
    reading: Option<ClockReading>,
    display: DisplaySnapshot,
    segments: Vec<SegmentState>,
    schedule: EventSchedule,
    active_event: Option<ActiveEvent>,
}

impl ClockState {
    pub fn settings(&self) -> ClockSettings {
        ClockSettings {
            time_format: self.config.time_format,
            event_profile: self.config.event_profile,
            events: self.config.events,
            marquee_preset: self.config.marquee_preset,
            marquee_message: self.config.marquee_message,
        }
    }

    fn configure(&mut self, settings: ClockSettings) {
        self.schedule
            .configure(settings.event_profile, settings.events);
        self.config.time_format = settings.time_format;
        self.config.event_profile = settings.event_profile;
        self.config.events = settings.events;
        self.config.marquee_preset = settings.marquee_preset;
        self.config.marquee_message = settings.marquee_message;
        if let Some(reading) = self.reading {
            self.apply_reading(reading, false);
        }
    }

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
        let aspect_ratio = normalize_aspect_ratio(aspect_ratio);
        if self.config.aspect_ratio == aspect_ratio {
            return;
        }
        self.config.aspect_ratio = aspect_ratio;
        // A resize changes both anchors and floor geometry. Recover immediately
        // instead of leaving bodies in the old arena or teleporting colliders.
        if self.active_event.is_some() {
            self.finish_event();
        }
    }

    pub fn lifecycle(&self) -> EventLifecycle {
        self.schedule.lifecycle
    }
    pub fn event_kind(&self) -> Option<ClockEventKind> {
        self.active_event.as_ref().map(ActiveEvent::kind)
    }
    pub fn event_phase(&self) -> Option<EventPhase> {
        self.active_event.as_ref().map(ActiveEvent::phase)
    }
    pub fn phase_tick(&self) -> u64 {
        self.active_event
            .as_ref()
            .map_or(self.schedule.lifecycle_tick, ActiveEvent::phase_tick)
    }
    pub fn palette(&self) -> DigitPalette {
        self.active_event
            .as_ref()
            .map_or_else(DigitPalette::default, ActiveEvent::palette)
    }
    pub fn event_enabled(&self, kind: ClockEventKind) -> bool {
        self.config.events.enabled(kind)
    }
    pub fn event_ready_at_tick(&self, kind: ClockEventKind) -> u64 {
        self.schedule.ready_at[kind as usize]
    }
    pub fn simulation_tick(&self) -> u64 {
        self.schedule.tick
    }
    pub fn event_id(&self) -> u64 {
        self.schedule.event_id
    }
    pub fn next_event_tick(&self) -> Option<u64> {
        self.schedule.next_event_tick
    }
    pub fn event_profile(&self) -> ClockEventProfile {
        self.config.event_profile
    }
    pub fn body_count(&self) -> usize {
        self.active_event
            .as_ref()
            .map_or(0, |event| event.physics_counts().0)
    }
    pub fn collider_count(&self) -> usize {
        self.active_event
            .as_ref()
            .map_or(0, |event| event.physics_counts().1)
    }
    pub fn meltdown_state(&self) -> Option<engine_common::ClockMeltdownState> {
        match self.active_event.as_ref()? {
            ActiveEvent::Meltdown(event) => Some(event.diagnostics()),
            _ => None,
        }
    }

    pub fn duck_state(&self) -> Option<engine_common::ClockDuckState> {
        match self.active_event.as_ref()? {
            ActiveEvent::Duck(event) => Some(event.diagnostics()),
            _ => None,
        }
    }
    pub fn marquee_state(&self) -> Option<engine_common::ClockMarqueeState> {
        match self.active_event.as_ref()? {
            ActiveEvent::Marquee(event) => Some(event.diagnostics()),
            _ => None,
        }
    }
    pub fn digit_slide_state(&self) -> Option<engine_common::ClockDigitSlideState> {
        match self.active_event.as_ref()? {
            ActiveEvent::DigitSlide(event) => Some(event.diagnostics()),
            _ => None,
        }
    }
    pub fn can_trigger_event(&self) -> bool {
        self.reading.is_some() && self.lifecycle() == EventLifecycle::Idle
    }

    fn trigger_event(&mut self, kind: ClockEventKind) {
        if self.can_trigger_event() {
            self.start_event(kind);
        }
    }

    fn preview_event(&mut self, kind: ClockEventKind) {
        if self.reading.is_some() {
            // A deliberate preview replaces an event, including its temporary
            // physics/appearance, but keeps the instance, clock and event IDs.
            self.finish_event();
            self.start_event(kind);
        }
    }

    fn start_event(&mut self, kind: ClockEventKind) {
        self.start_event_from(kind, None);
    }

    fn start_event_from(
        &mut self,
        kind: ClockEventKind,
        previous_display: Option<DisplaySnapshot>,
    ) {
        let seed = self.schedule.start(kind);
        let layout = Layout::new(self.aspect_ratio());
        self.active_event = Some(ActiveEvent::new(
            kind,
            EventContext {
                segments: &mut self.segments,
                display: self.display,
                layout,
            },
            seed,
            self.config,
            previous_display,
        ));
    }

    fn finish_event(&mut self) {
        if let Some(event) = self.active_event.take() {
            self.schedule.finish(event.kind());
        }
        for segment in &mut self.segments {
            segment.representation = SegmentRepresentation::Anchored;
        }
        digits::apply_snapshot(&mut self.segments, self.display);
    }

    fn advance_tick(&mut self) {
        self.schedule.advance_tick();
        let layout = Layout::new(self.aspect_ratio());
        if let Some(event) = &mut self.active_event {
            if event.step(EventContext {
                segments: &mut self.segments,
                display: self.display,
                layout,
            }) {
                self.finish_event();
            }
        } else if let Some(kind) = self.schedule.due_event(self.reading.is_some()) {
            self.trigger_event(kind);
        }
    }

    fn apply_reading(&mut self, reading: ClockReading, animate: bool) {
        let previous = self.display;
        let next = digits::snapshot(reading, self.config.time_format);
        let minute_changed = previous.digits != next.digits;
        // Only near-contiguous forward readings animate. Initial sync, skipped
        // minutes, backwards corrections and paused control synchronization snap
        // straight to the truth; there is no backlog of stale transitions.
        let slide = animate
            && minute_changed
            && self.reading.is_some_and(|old| {
                let seconds = |r: ClockReading| {
                    u32::from(r.hour()) * 3600 + u32::from(r.minute()) * 60 + u32::from(r.second())
                };
                let delta = (seconds(reading) + 86400 - seconds(old)) % 86400;
                (1..=3).contains(&delta)
            });
        self.reading = Some(reading);
        self.display = next;
        // A second changed target supersedes a slide instead of letting old
        // digits finish over a newer reading. Ordinary seconds don't restart it.
        if matches!(self.active_event, Some(ActiveEvent::DigitSlide(_))) && minute_changed {
            self.finish_event();
        }
        if let Some(ActiveEvent::Marquee(event)) = &mut self.active_event {
            event.synchronize(self.display);
        }
        if !self
            .active_event
            .as_ref()
            .is_some_and(ActiveEvent::holds_lit_segments)
        {
            digits::apply_snapshot(&mut self.segments, self.display);
        }
        if slide && let Some(kind) = self.schedule.time_change_event() {
            self.start_event_from(kind, Some(previous));
        }
    }
}

pub struct ClockScenario;

impl Scenario for ClockScenario {
    type State = ClockState;
    type Config = ClockConfig;

    fn init(config: Self::Config, seed: u64) -> Self::State {
        ClockState {
            config: config.normalized(),
            reading: None,
            display: DisplaySnapshot::unsynchronized(),
            segments: digits::create_segments(),
            schedule: EventSchedule::new(config.event_profile, config.events, seed),
            active_event: None,
        }
    }

    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        for action in actions.iter().filter_map(ClockAction::decode) {
            match action {
                ClockAction::SetReading(reading) => state.apply_reading(reading, !dt.is_zero()),
                ClockAction::TriggerEvent(kind) => state.trigger_event(kind),
                ClockAction::Configure(settings) => state.configure(settings),
                ClockAction::PreviewEvent(kind) => state.preview_event(kind),
            }
        }
        // The fixed-timestep host supplies one tick per call. Zero duration is
        // used to synchronize inputs/control actions without advancing physics.
        if !dt.is_zero() {
            state.advance_tick();
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
        TickModel::FixedTimestep { hz: FIXED_HZ }
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
                vec![2, 0, 24, 0, 0],
            )),
            None
        );
        assert_eq!(
            ClockAction::decode(&Action::scenario(
                CLOCK_ACTION_SET_READING,
                vec![1, 0, 19, 42, 7],
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
                ..ClockConfig::default()
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
