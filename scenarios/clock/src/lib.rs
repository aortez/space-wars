//! Deterministic low-resolution clock scenario.

#[cfg(test)]
mod autonomous_tests;
mod calendar;
#[cfg(test)]
mod calendar_tests;
mod crow;
#[cfg(test)]
mod crow_tests;
pub use calendar::ClockDate;
#[cfg(test)]
mod digit_slide_tests;
mod digits;
#[cfg(test)]
mod event_tests;
mod events;
mod floor;
#[cfg(test)]
mod floor_tests;
#[cfg(test)]
mod input_tests;
mod layout;
#[cfg(test)]
mod live_tests;
mod meridiem;
mod physics;
mod player;
#[cfg(test)]
mod player_tests;
mod presentation;
mod rain;
mod render;
pub mod water_fixture;

use std::time::Duration;

pub use digits::{
    DIGIT_SLOT_COUNT, DisplaySnapshot, GridCell, SegmentId, SegmentKind, SegmentRepresentation,
    SegmentState,
};

use engine_common::{
    Action, ClockEventKind, ClockEventProfile, ClockEvents, ClockMarqueeMessage,
    ClockMarqueePreset, ClockRainAmount, ClockSettings, ClockTimeFormat, Observation, RenderFrame,
    Scenario, StepResult, TickModel,
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

pub const CLOCK_ACTION_VERSION: u16 = 8;
pub const CLOCK_ACTION_SET_READING: u32 = 1;
pub const CLOCK_ACTION_TRIGGER_EVENT: u32 = 3;
pub const CLOCK_ACTION_CONFIGURE: u32 = 4;
pub const CLOCK_ACTION_PREVIEW_EVENT: u32 = 5;
pub const CLOCK_ACTION_NEXT_EVENT: u32 = 6;
pub const CLOCK_ACTION_TOGGLE_PLAYER_DUCK: u32 = 7;
pub const CLOCK_ACTION_PLAYER_DUCK_INPUT: u32 = 8;
pub use player::ClockDuckInput;
pub const CLOCK_OBSERVATION_VERSION: u16 = 2;

const DEFAULT_ASPECT_RATIO: f32 = 800.0 / 480.0;
const MIN_ASPECT_RATIO: f32 = 0.25;
const MAX_ASPECT_RATIO: f32 = 4.0;
const MAX_CONFIGURE_BYTES: usize = 8 + engine_common::MAX_CLOCK_MESSAGE_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockReading {
    hour: u8,
    minute: u8,
    second: u8,
    date: Option<ClockDate>,
}

impl ClockReading {
    pub const fn new(hour: u8, minute: u8, second: u8) -> Option<Self> {
        if hour < 24 && minute < 60 && second < 60 {
            Some(Self {
                hour,
                minute,
                second,
                date: None,
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

    pub const fn with_date(mut self, date: ClockDate) -> Self {
        self.date = Some(date);
        self
    }

    pub const fn date(self) -> Option<ClockDate> {
        self.date
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockAction {
    SetReading(ClockReading),
    TriggerEvent(ClockEventKind),
    Configure(ClockSettings),
    PreviewEvent(ClockEventKind),
    NextEvent,
    TogglePlayerDuck(u8),
    PlayerDuckInput(ClockDuckInput),
}

impl ClockAction {
    pub fn next_event() -> Action {
        Action::scenario(
            CLOCK_ACTION_NEXT_EVENT,
            CLOCK_ACTION_VERSION.to_le_bytes().to_vec(),
        )
    }

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
                | (u8::from(settings.events.digit_slide) << 5)
                | (u8::from(settings.events.rain) << 6)
                | (u8::from(settings.events.crow) << 7),
        );
        payload.push(settings.marquee_preset as u8);
        payload.push(settings.rain_amount as u8);
        payload.push(u8::from(settings.show_date));
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
        let mut payload = Vec::with_capacity(9);
        payload.extend_from_slice(&CLOCK_ACTION_VERSION.to_le_bytes());
        payload.extend_from_slice(&[reading.hour, reading.minute, reading.second]);
        if let Some(date) = reading.date {
            let [year, month, day] = date.parts();
            payload.extend_from_slice(&year.to_le_bytes());
            payload.extend_from_slice(&[month as u8, day as u8]);
        }
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
            (CLOCK_ACTION_TOGGLE_PLAYER_DUCK, 3) if (1..=2).contains(&payload[2]) => {
                Some(Self::TogglePlayerDuck(payload[2]))
            }
            (CLOCK_ACTION_PLAYER_DUCK_INPUT, 14) => {
                ClockDuckInput::decode(&payload[2..]).map(Self::PlayerDuckInput)
            }
            (CLOCK_ACTION_NEXT_EVENT, 2) => Some(Self::NextEvent),
            (CLOCK_ACTION_SET_READING, 5) => {
                ClockReading::new(payload[2], payload[3], payload[4]).map(Self::SetReading)
            }
            (CLOCK_ACTION_SET_READING, 9) => {
                let date = ClockDate::new(
                    u16::from_le_bytes([payload[5], payload[6]]),
                    payload[7],
                    payload[8],
                )?;
                Some(Self::SetReading(
                    ClockReading::new(payload[2], payload[3], payload[4])?.with_date(date),
                ))
            }
            (CLOCK_ACTION_TRIGGER_EVENT, 3) => ClockEventKind::ALL
                .into_iter()
                .find(|kind| *kind as u8 == payload[2])
                .map(Self::TriggerEvent),
            (CLOCK_ACTION_PREVIEW_EVENT, 3) => ClockEventKind::ALL
                .into_iter()
                .find(|kind| *kind as u8 == payload[2])
                .map(Self::PreviewEvent),
            (CLOCK_ACTION_CONFIGURE, 9..=MAX_CONFIGURE_BYTES) if payload[7] <= 1 => {
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
                        rain: payload[4] & 64 != 0,
                        crow: payload[4] & 128 != 0,
                    },
                    marquee_preset: *ClockMarqueePreset::ALL.get(usize::from(payload[5]))?,
                    rain_amount: *ClockRainAmount::ALL.get(usize::from(payload[6]))?,
                    show_date: payload[7] != 0,
                    marquee_message: std::str::from_utf8(&payload[8..]).ok()?.parse().ok()?,
                }))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ClockWaterLab {
    #[default]
    Off,
    Cascade,
    Displacement,
    DisplacementControl,
    Floating,
    FloatingControl,
    Sinking,
    Rotating,
    RotatingControl,
    Multiple,
    MultipleControl,
    Spilling,
    SpillingControl,
}

impl ClockWaterLab {
    pub fn from_override(value: Option<&str>) -> Self {
        match value {
            Some("1" | "cascade") => Self::Cascade,
            Some("displacement") => Self::Displacement,
            Some("displacement-control") => Self::DisplacementControl,
            Some("floating") => Self::Floating,
            Some("floating-control") => Self::FloatingControl,
            Some("sinking") => Self::Sinking,
            Some("rotating") => Self::Rotating,
            Some("rotating-control") => Self::RotatingControl,
            Some("multiple") => Self::Multiple,
            Some("multiple-control") => Self::MultipleControl,
            Some("spilling") => Self::Spilling,
            Some("spilling-control") => Self::SpillingControl,
            _ => Self::Off,
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "normal",
            Self::Cascade => "collecting-pool",
            Self::Displacement => "displacement",
            Self::DisplacementControl => "displacement-control",
            Self::Floating => "floating",
            Self::FloatingControl => "floating-control",
            Self::Sinking => "sinking",
            Self::Rotating => "rotating",
            Self::RotatingControl => "rotating-control",
            Self::Multiple => "multiple",
            Self::MultipleControl => "multiple-control",
            Self::Spilling => "spilling",
            Self::SpillingControl => "spilling-control",
        }
    }
    pub const fn is_tank(self) -> bool {
        matches!(self, Self::Displacement | Self::DisplacementControl) || self.is_dynamic_tank()
    }
    pub const fn is_dynamic_tank(self) -> bool {
        matches!(self, Self::Floating | Self::FloatingControl | Self::Sinking)
            || self.is_rotating_tank()
            || self.is_multiple_tank()
    }
    pub const fn is_rotating_tank(self) -> bool {
        matches!(self, Self::Rotating | Self::RotatingControl)
    }
    pub const fn is_multiple_tank(self) -> bool {
        matches!(self, Self::Multiple | Self::MultipleControl) || self.is_spilling_tank()
    }
    pub const fn is_spilling_tank(self) -> bool {
        matches!(self, Self::Spilling | Self::SpillingControl)
    }
    pub const fn has_displacement(self) -> bool {
        matches!(
            self,
            Self::Displacement
                | Self::Floating
                | Self::Sinking
                | Self::Rotating
                | Self::Multiple
                | Self::Spilling
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockConfig {
    pub aspect_ratio: f32,
    pub duck_debug_overlay: bool,
    /// Development-only water fixtures using the Meltdown lifecycle.
    pub water_lab: ClockWaterLab,
    /// None chooses a seeded personality once per Duck event.
    pub duck_jump_profile: Option<engine_common::ClockDuckJumpProfile>,
    /// None selects a seeded course pattern, independently of personality.
    pub duck_course_pattern: Option<engine_common::ClockDuckCoursePattern>,
    pub time_format: ClockTimeFormat,
    pub show_date: bool,
    pub event_profile: ClockEventProfile,
    pub events: ClockEvents,
    pub marquee_preset: ClockMarqueePreset,
    pub marquee_message: ClockMarqueeMessage,
    pub rain_amount: ClockRainAmount,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            aspect_ratio: DEFAULT_ASPECT_RATIO,
            duck_debug_overlay: false,
            water_lab: ClockWaterLab::Off,
            duck_jump_profile: None,
            duck_course_pattern: None,
            time_format: ClockTimeFormat::TwentyFourHour,
            show_date: false,
            event_profile: ClockEventProfile::default(),
            events: ClockEvents::default(),
            marquee_preset: ClockMarqueePreset::default(),
            marquee_message: ClockMarqueeMessage::default(),
            rain_amount: ClockRainAmount::default(),
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
            show_date: self.show_date,
            event_profile: self.event_profile,
            events: self.events,
            marquee_preset: self.marquee_preset,
            marquee_message: self.marquee_message,
            rain_amount: self.rain_amount,
        }
    }
}

pub struct ClockState {
    config: ClockConfig,
    reading: Option<ClockReading>,
    date_label: Option<String>,
    display: DisplaySnapshot,
    segments: Vec<SegmentState>,
    schedule: EventSchedule,
    active_event: Option<ActiveEvent>,
    floor: floor::FloorManager,
    last_started_event: Option<ClockEventKind>,
    event_notice: Option<(&'static str, u64)>,
    // One physical visit, independent of the event scheduler and command source.
    // Automatic ducks keep their own bounded lifetime; players may stay longer.
    duck_visit: Option<Box<events::duck::DuckEvent>>,
    crow_visit: Option<crow::CrowVisit>,
    player_duck_sequence: u64,
    player_seed: u64,
}

impl ClockState {
    pub fn settings(&self) -> ClockSettings {
        ClockSettings {
            time_format: self.config.time_format,
            show_date: self.config.show_date,
            event_profile: self.config.event_profile,
            events: self.config.events,
            marquee_preset: self.config.marquee_preset,
            marquee_message: self.config.marquee_message,
            rain_amount: self.config.rain_amount,
        }
    }

    fn configure(&mut self, settings: ClockSettings) {
        self.config.time_format = settings.time_format;
        self.config.show_date = settings.show_date;
        self.config.event_profile = settings.event_profile;
        self.config.events = settings.events;
        self.config.marquee_preset = settings.marquee_preset;
        self.config.marquee_message = settings.marquee_message;
        self.config.rain_amount = settings.rain_amount;
        self.sync_event_schedule();
        if let Some(reading) = self.reading {
            self.apply_reading(reading, false);
        }
    }

    pub fn reading(&self) -> Option<ClockReading> {
        self.reading
    }

    pub fn date_label(&self) -> Option<&str> {
        self.date_label.as_deref()
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

    pub fn floor_mode(&self) -> engine_common::ClockFloorMode {
        self.floor.mode()
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        let aspect_ratio = normalize_aspect_ratio(aspect_ratio);
        if self.config.aspect_ratio == aspect_ratio {
            return;
        }
        self.config.aspect_ratio = aspect_ratio;
        // A resize changes both anchors and floor geometry. Recover immediately
        // instead of leaving bodies in the old arena or teleporting colliders.
        self.finish_duck_visit();
        self.finish_crow_visit();
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
            + self
                .duck_visit
                .as_ref()
                .map_or(0, |duck| duck.physics_counts().0)
    }
    pub fn collider_count(&self) -> usize {
        self.active_event
            .as_ref()
            .map_or(0, |event| event.physics_counts().1)
            + self
                .duck_visit
                .as_ref()
                .map_or(0, |duck| duck.physics_counts().1)
    }
    pub fn meltdown_state(&self) -> Option<engine_common::ClockMeltdownState> {
        match self.active_event.as_ref()? {
            ActiveEvent::Meltdown(event) => Some(event.diagnostics()),
            _ => None,
        }
    }

    pub fn duck_state(&self) -> Option<engine_common::ClockDuckState> {
        let duck = self.duck_visit.as_ref()?;
        duck.player_session().is_none().then(|| duck.diagnostics())
    }
    pub fn marquee_state(&self) -> Option<engine_common::ClockMarqueeState> {
        match self.active_event.as_ref()? {
            ActiveEvent::Marquee(event) => Some(event.diagnostics()),
            _ => None,
        }
    }
    pub fn crow_state(&self) -> Option<engine_common::ClockCrowState> {
        self.crow_visit.as_ref().map(crow::CrowVisit::diagnostics)
    }

    pub fn event_blocked_by_crow(&self, kind: ClockEventKind) -> bool {
        kind == ClockEventKind::Crow && self.crow_visit.is_some()
    }

    fn event_blocked(&self, kind: ClockEventKind) -> bool {
        self.event_blocked_by_duck(kind) || self.event_blocked_by_crow(kind)
    }

    fn finish_crow_visit(&mut self) {
        if self.crow_visit.take().is_some() {
            self.schedule.retire(ClockEventKind::Crow);
            self.sync_event_schedule();
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

    pub fn rain_state(&self) -> Option<engine_common::ClockRainState> {
        match self.active_event.as_ref()? {
            ActiveEvent::Rain(event) => Some(event.diagnostics()),
            _ => None,
        }
    }

    fn trigger_event(&mut self, kind: ClockEventKind) {
        if self.can_trigger_event() && !self.event_blocked(kind) {
            self.start_event(kind);
        }
    }

    fn preview_event(&mut self, kind: ClockEventKind) {
        if self.reading.is_some() {
            if self.event_blocked_by_crow(kind) {
                self.event_notice = Some(("Crow is already visiting", self.schedule.tick + 120));
                return;
            }
            if self.event_blocked_by_duck(kind) {
                self.event_notice = Some((
                    if self.player_duck_session().is_some() {
                        "Dismiss your duck to preview this event"
                    } else {
                        "Wait for the duck, or take control and dismiss it"
                    },
                    self.schedule.tick + 2 * u64::from(FIXED_HZ),
                ));
                return;
            }
            // A deliberate preview replaces an event, including its temporary
            // physics/appearance, but keeps the instance, clock and event IDs.
            if kind != ClockEventKind::Crow {
                self.finish_event();
            }
            self.start_event(kind);
        }
    }

    fn next_event(&mut self) {
        if self.reading.is_none() {
            return;
        }
        let kinds = ClockEventKind::ALL;
        let start = self.last_started_event.map_or(0, |kind| kind as usize + 1);
        let next = (0..kinds.len())
            .map(|offset| kinds[(start + offset) % kinds.len()])
            .find(|kind| self.config.events.enabled(*kind) && !self.event_blocked(*kind));
        let message = if let Some(kind) = next {
            // The preview path owns cancellation/restoration for every event.
            // Manual cycling ignores automatic cooldowns, not enabled switches.
            self.preview_event(kind);
            kind.label()
        } else if self.duck_visit.is_some()
            && ClockEventKind::ALL
                .into_iter()
                .any(|kind| self.config.events.enabled(kind))
        {
            "Waiting for the duck to leave"
        } else if self.crow_visit.is_some() && self.config.events.crow {
            "Crow is already visiting"
        } else {
            "No events enabled"
        };
        self.event_notice = Some((message, self.schedule.tick + 2 * u64::from(FIXED_HZ)));
    }

    fn start_event(&mut self, kind: ClockEventKind) {
        self.start_event_from(kind, None);
    }

    fn start_event_from(
        &mut self,
        kind: ClockEventKind,
        previous_display: Option<DisplaySnapshot>,
    ) {
        debug_assert!(!self.event_blocked(kind));
        self.last_started_event = Some(kind);
        self.event_notice = None;
        if kind == ClockEventKind::Crow {
            let seed = self.schedule.admit_resident(kind);
            self.crow_visit = Some(crow::CrowVisit::new(
                Layout::new(self.aspect_ratio()),
                seed,
                self.schedule.event_id,
                &self.segments,
                self.event_kind(),
            ));
            self.sync_event_schedule();
            return;
        }
        let seed = self.schedule.start(kind);
        let layout = Layout::new(self.aspect_ratio());
        if kind == ClockEventKind::Duck {
            let mut duck =
                events::duck::DuckEvent::new_course(layout, seed, self.config.duck_course_pattern);
            duck.select_jump_profile(self.config.duck_jump_profile);
            self.floor.acquire_visit();
            self.duck_visit = Some(Box::new(duck));
            // Admission uses the ordinary cadence/cooldown, but does not hold
            // the timed-event slot for the actor's entire 35-second visit.
            self.schedule.finish(kind);
            self.sync_event_schedule();
            return;
        }
        if kind == ClockEventKind::Rain
            && let Some(duck) = &self.duck_visit
        {
            self.floor.acquire_visit_event(kind);
            let rain = if let Some(floor) = duck.responsive_floor() {
                rain::RainEvent::on_responsive_floor(
                    layout,
                    seed,
                    self.config.rain_amount,
                    self.display,
                    floor.clone(),
                )
            } else {
                rain::RainEvent::on_course(
                    events::duck::arena::CourseGeometry::from_duck(duck),
                    seed,
                    self.config.rain_amount,
                    self.display,
                )
            };
            self.active_event = Some(ActiveEvent::Rain(Box::new(rain)));
            return;
        }
        if matches!(kind, ClockEventKind::Falling | ClockEventKind::Meltdown)
            && let Some(duck) = &mut self.duck_visit
        {
            self.floor.acquire_visit_event(kind);
            let context = EventContext {
                segments: &mut self.segments,
                display: self.display,
                layout,
                floor: self.floor.geometry(layout),
            };
            self.active_event = Some(match kind {
                ClockEventKind::Falling => ActiveEvent::Falling(
                    events::falling::FallingEvent::with_visit(context, seed, duck),
                ),
                ClockEventKind::Meltdown => ActiveEvent::Meltdown(Box::new(
                    events::meltdown::MeltdownEvent::with_visit(context, seed, duck),
                )),
                _ => unreachable!(),
            });
            return;
        }
        self.floor.acquire(kind);
        self.active_event = Some(ActiveEvent::new(
            kind,
            EventContext {
                segments: &mut self.segments,
                display: self.display,
                layout,
                floor: self.floor.geometry(layout),
            },
            seed,
            self.config,
            previous_display,
        ));
    }

    fn finish_event(&mut self) {
        if let Some(mut event) = self.active_event.take() {
            let kind = event.kind();
            event.release_arena(self.duck_visit.as_deref_mut());
            if let ActiveEvent::Rain(rain) = &event
                && let Some(floor) = rain.responsive_floor()
                && let Some(duck) = &mut self.duck_visit
            {
                duck.sync_responsive_floor(floor);
            }
            drop(event);
            self.schedule.finish(kind);
            self.floor.release(kind);
        }
        // Visual-event cleanup never releases the visit's physical arena.
        for segment in &mut self.segments {
            segment.representation = SegmentRepresentation::Anchored;
        }
        digits::apply_snapshot(&mut self.segments, self.display);
    }

    fn advance_tick(&mut self) {
        self.schedule.advance_tick();
        if self
            .event_notice
            .is_some_and(|(_, until)| self.schedule.tick >= until)
        {
            self.event_notice = None;
        }
        let layout = Layout::new(self.aspect_ratio());
        let panels_advanced = matches!(&self.active_event, Some(ActiveEvent::Rain(rain)) if rain.responsive_floor().is_some());
        if let Some(ActiveEvent::Rain(rain)) = &mut self.active_event {
            rain.set_duck_hull(self.duck_visit.as_ref().and_then(|d| d.clearance_hull()));
        }
        let visit_advanced = if self.active_event.is_some() {
            self.step_active_event(layout)
        } else {
            if let Some(kind) = self.schedule.due_event(self.reading.is_some()) {
                self.trigger_event(kind);
            }
            // A newly inserted shared batch must be advanced/read back with
            // the character this tick too; otherwise the rendered bars lag
            // their colliders until the first regular event step.
            if self
                .active_event
                .as_ref()
                .is_some_and(ActiveEvent::shares_visit_arena)
            {
                self.step_active_event(layout)
            } else {
                false
            }
        };
        // Water advances once, then the one duck mechanics world samples it.
        // No second character world or frame-rate dependent force application.
        let water = match &self.active_event {
            Some(ActiveEvent::Rain(rain)) => {
                if let Some(duck) = &mut self.duck_visit
                    && let Some(floor) = rain.responsive_floor()
                {
                    duck.sync_responsive_floor(floor);
                }
                Some(&rain.water)
            }
            _ => None,
        };
        if let Some(duck) = &mut self.duck_visit
            && if visit_advanced {
                duck.visit_finished()
            } else {
                duck.step_with_environment(water, panels_advanced || water.is_some())
            }
        {
            self.finish_duck_visit();
        }
        let kind = self.event_kind();
        if let Some(crow) = &mut self.crow_visit
            && crow.step(&self.segments, kind)
        {
            self.finish_crow_visit();
        }
    }

    /// Reports whether the event already stepped the visit's mechanics world.
    /// This remains true on its final tick, after cleanup removes the event.
    fn step_active_event(&mut self, layout: Layout) -> bool {
        let event = self.active_event.as_mut().expect("active event");
        let visit_advanced = event.shares_visit_arena();
        let context = EventContext {
            segments: &mut self.segments,
            display: self.display,
            layout,
            floor: self.floor.geometry(layout),
        };
        let finished = match event {
            ActiveEvent::Falling(falling) => falling.step(context, self.duck_visit.as_deref_mut()),
            ActiveEvent::Meltdown(meltdown) => {
                meltdown.step_with_visit(context, self.duck_visit.as_deref_mut())
            }
            _ => event.step(context),
        };
        if finished {
            self.finish_event();
        }
        visit_advanced
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
                let delta = match (old.date, reading.date) {
                    (Some(old_date), Some(date)) => {
                        i64::from(date.ordinal() - old_date.ordinal()) * 86400
                            + i64::from(seconds(reading))
                            - i64::from(seconds(old))
                    }
                    // Time-only fixtures retain the midnight wrap behavior.
                    (None, None) => i64::from((seconds(reading) + 86400 - seconds(old)) % 86400),
                    _ => return false,
                };
                (1..=3).contains(&delta)
            });
        if self.reading.and_then(ClockReading::date) != reading.date {
            self.date_label = reading.date.map(ClockDate::label);
        }
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
        if let Some(ActiveEvent::Rain(event)) = &mut self.active_event {
            // A control-only update may transfer wet support into free water,
            // but never advances time, consumes rain RNG, or integrates motion.
            event.synchronize(self.display, &mut self.segments);
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
            date_label: None,
            display: DisplaySnapshot::unsynchronized(),
            segments: digits::create_segments(),
            schedule: EventSchedule::new(config.event_profile, config.events, seed),
            active_event: None,
            floor: floor::FloorManager::default(),
            last_started_event: None,
            event_notice: None,
            duck_visit: None,
            crow_visit: None,
            player_duck_sequence: 0,
            player_seed: seed ^ 0x504c_4159_4455_434b,
        }
    }

    fn step(state: &mut Self::State, actions: &[Action], dt: Duration) -> StepResult {
        for action in actions.iter().filter_map(ClockAction::decode) {
            match action {
                ClockAction::SetReading(reading) => state.apply_reading(reading, !dt.is_zero()),
                ClockAction::TriggerEvent(kind) => state.trigger_event(kind),
                ClockAction::Configure(settings) => state.configure(settings),
                ClockAction::PreviewEvent(kind) => state.preview_event(kind),
                ClockAction::NextEvent => state.next_event(),
                ClockAction::TogglePlayerDuck(player) => state.toggle_player_duck(player),
                ClockAction::PlayerDuckInput(input) => state.apply_player_duck_input(input),
            }
        }
        // The fixed-timestep host supplies one tick per call. Zero duration is
        // used to synchronize inputs/control actions without advancing physics.
        if dt.is_zero() {
            let kind = state.event_kind();
            if let Some(crow) = &mut state.crow_visit {
                crow.synchronize(&state.segments, kind);
            }
        } else {
            // The visitor revalidates support after the timed event updates it.
            state.advance_tick();
        }
        StepResult::default()
    }

    fn observe(state: &Self::State) -> Observation {
        let mut payload = Vec::with_capacity(12);
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
        payload.push(u8::from(state.config.show_date));
        let [year, month, day] = state
            .reading
            .and_then(ClockReading::date)
            .map(ClockDate::parts)
            .unwrap_or([0; 3]);
        payload.extend_from_slice(&year.to_le_bytes());
        payload.extend_from_slice(&[month as u8, day as u8]);
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

    #[test]
    fn water_lab_overrides_are_explicit_and_normal_startup_is_unchanged() {
        for (value, expected) in [
            ("1", ClockWaterLab::Cascade),
            ("cascade", ClockWaterLab::Cascade),
            ("displacement", ClockWaterLab::Displacement),
            ("displacement-control", ClockWaterLab::DisplacementControl),
            ("floating", ClockWaterLab::Floating),
            ("floating-control", ClockWaterLab::FloatingControl),
            ("sinking", ClockWaterLab::Sinking),
            ("rotating", ClockWaterLab::Rotating),
            ("rotating-control", ClockWaterLab::RotatingControl),
            ("multiple", ClockWaterLab::Multiple),
            ("multiple-control", ClockWaterLab::MultipleControl),
            ("spilling", ClockWaterLab::Spilling),
            ("spilling-control", ClockWaterLab::SpillingControl),
        ] {
            assert_eq!(ClockWaterLab::from_override(Some(value)), expected);
        }
        for value in [None, Some(""), Some("0"), Some("unknown")] {
            assert_eq!(ClockWaterLab::from_override(value), ClockWaterLab::Off);
        }
        assert_eq!(ClockConfig::default().water_lab, ClockWaterLab::Off);
    }

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
            vec![2, 0, 0, 255, 255, 255, 12, 0, 0, 0, 0, 0]
        );
        ClockScenario::step(
            &mut state,
            &[ClockAction::set_reading(reading(23, 59, 58))],
            Duration::ZERO,
        );
        assert_eq!(
            ClockScenario::observe(&state).payload,
            vec![2, 0, 1, 23, 59, 58, 12, 0, 0, 0, 0, 0]
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
