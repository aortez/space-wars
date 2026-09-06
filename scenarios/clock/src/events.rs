mod color_cycle;
mod falling;
#[cfg(test)]
mod tests;

use engine_common::{ClockEventKind, ClockEventProfile, ClockEvents};
use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{DisplaySnapshot, SegmentState, layout::Layout};
use color_cycle::ColorCycle;
pub use color_cycle::{COLOR_CYCLE_TICKS, DigitPalette};
use falling::FallingEvent;
pub use falling::{FALLING_TICKS, REFORMING_TICKS};

pub const FIXED_HZ: u32 = 60;
pub const COOLDOWN_TICKS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLifecycle {
    Idle,
    Active,
    Cooldown,
}

impl EventLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Active => "active",
            Self::Cooldown => "cooldown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    Falling,
    Reforming,
    Cycling,
}

impl EventPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Falling => "falling",
            Self::Reforming => "reforming",
            Self::Cycling => "cycling",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventEffect {
    DigitGeometry,
    Appearance,
}

impl EventEffect {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DigitGeometry => "digit-geometry",
            Self::Appearance => "appearance",
        }
    }
}

pub struct EventDefinition {
    pub kind: ClockEventKind,
    pub effect: EventEffect,
    pub duration_ticks: u64,
    /// Automatic reuse delay measured from completion or cancellation.
    pub cooldown_ticks: u64,
}

pub const EVENT_CATALOG: [EventDefinition; ClockEventKind::ALL.len()] = [
    EventDefinition {
        kind: ClockEventKind::Falling,
        effect: EventEffect::DigitGeometry,
        duration_ticks: FALLING_TICKS + REFORMING_TICKS,
        cooldown_ticks: 30 * 60,
    },
    EventDefinition {
        kind: ClockEventKind::ColorCycle,
        effect: EventEffect::Appearance,
        duration_ticks: COLOR_CYCLE_TICKS,
        cooldown_ticks: 15 * 60,
    },
];

pub(super) struct EventContext<'a> {
    pub segments: &'a mut [SegmentState],
    pub display: DisplaySnapshot,
    pub layout: Layout,
}

/// An event owns its local phase and temporary resources. Dropping the variant
/// releases them; ClockState restores the face on every completion/cancellation.
pub(super) enum ActiveEvent {
    Falling(FallingEvent),
    ColorCycle(ColorCycle),
}

impl ActiveEvent {
    pub fn new(kind: ClockEventKind, context: EventContext<'_>, seed: u64) -> Self {
        match kind {
            ClockEventKind::Falling => Self::Falling(FallingEvent::new(context, seed)),
            ClockEventKind::ColorCycle => Self::ColorCycle(ColorCycle::default()),
        }
    }

    pub fn kind(&self) -> ClockEventKind {
        match self {
            Self::Falling(_) => ClockEventKind::Falling,
            Self::ColorCycle(_) => ClockEventKind::ColorCycle,
        }
    }

    /// Returns true when the event has finished.
    pub fn step(&mut self, context: EventContext<'_>) -> bool {
        match self {
            Self::Falling(event) => event.step(context),
            Self::ColorCycle(event) => event.step(),
        }
    }

    pub fn phase(&self) -> EventPhase {
        match self {
            Self::Falling(event) => event.phase(),
            Self::ColorCycle(_) => EventPhase::Cycling,
        }
    }

    pub fn phase_tick(&self) -> u64 {
        match self {
            Self::Falling(event) => event.phase_tick(),
            Self::ColorCycle(event) => event.tick,
        }
    }

    pub fn physics_counts(&self) -> (usize, usize) {
        match self {
            Self::Falling(event) => event.physics_counts(),
            Self::ColorCycle(_) => (0, 0),
        }
    }

    pub fn palette(&self) -> DigitPalette {
        match self {
            Self::ColorCycle(event) => event.palette(),
            _ => DigitPalette::default(),
        }
    }

    pub fn holds_lit_segments(&self) -> bool {
        self.phase() == EventPhase::Falling
    }
}

/// One global cadence, not a probability per event. All timing uses simulation
/// ticks, and animation randomness never consumes the scheduler's RNG stream.
pub(super) struct EventSchedule {
    pub lifecycle: EventLifecycle,
    pub lifecycle_tick: u64,
    pub tick: u64,
    pub event_id: u64,
    pub next_event_tick: Option<u64>,
    pub ready_at: [u64; ClockEventKind::ALL.len()],
    last_kind: Option<ClockEventKind>,
    rng: StdRng,
    seed: u64,
    profile: ClockEventProfile,
    enabled: ClockEvents,
}

impl EventSchedule {
    pub fn new(profile: ClockEventProfile, enabled: ClockEvents, seed: u64) -> Self {
        let mut result = Self {
            lifecycle: EventLifecycle::Idle,
            lifecycle_tick: 0,
            tick: 0,
            event_id: 0,
            next_event_tick: None,
            ready_at: [0; ClockEventKind::ALL.len()],
            last_kind: None,
            rng: StdRng::seed_from_u64(seed),
            seed,
            profile,
            enabled,
        };
        result.schedule_next();
        result
    }

    pub fn start(&mut self, kind: ClockEventKind) -> u64 {
        self.event_id += 1;
        self.last_kind = Some(kind);
        self.enter(EventLifecycle::Active);
        self.next_event_tick = None;
        self.seed
            .wrapping_add(self.event_id - 1)
            .wrapping_add((kind as u64) << 32)
    }

    pub fn configure(&mut self, profile: ClockEventProfile, enabled: ClockEvents) {
        if self.profile == profile && self.enabled == enabled {
            return;
        }
        self.profile = profile;
        self.enabled = enabled;
        // Do not interrupt an event or reset its cooldown. While idle, start
        // a fresh wait so switching from Off never fires a stale deadline.
        if self.lifecycle == EventLifecycle::Idle {
            self.schedule_next();
        }
    }

    pub fn finish(&mut self, kind: ClockEventKind) {
        self.ready_at[kind as usize] = self.tick + EVENT_CATALOG[kind as usize].cooldown_ticks;
        self.enter(EventLifecycle::Cooldown);
    }

    fn enter(&mut self, lifecycle: EventLifecycle) {
        self.lifecycle = lifecycle;
        self.lifecycle_tick = 0;
    }

    pub fn advance_tick(&mut self) {
        self.tick += 1;
        self.lifecycle_tick += 1;
        if self.lifecycle == EventLifecycle::Cooldown && self.lifecycle_tick >= COOLDOWN_TICKS {
            self.enter(EventLifecycle::Idle);
            self.schedule_next();
        }
    }

    pub fn due_event(&mut self, synchronized: bool) -> Option<ClockEventKind> {
        if !synchronized
            || self.lifecycle != EventLifecycle::Idle
            || self.next_event_tick.is_none_or(|at| self.tick < at)
        {
            return None;
        }
        let mut eligible: Vec<_> = ClockEventKind::ALL
            .into_iter()
            .filter(|kind| {
                self.enabled.enabled(*kind) && self.ready_at[*kind as usize] <= self.tick
            })
            .collect();
        if eligible.is_empty() {
            self.next_event_tick = ClockEventKind::ALL
                .into_iter()
                .filter(|kind| self.enabled.enabled(*kind))
                .map(|kind| self.ready_at[kind as usize])
                .min();
            return None;
        }
        if eligible.len() > 1 {
            eligible.retain(|kind| Some(*kind) != self.last_kind);
        }
        Some(eligible[self.rng.random_range(0..eligible.len())])
    }

    fn schedule_next(&mut self) {
        if !ClockEventKind::ALL
            .into_iter()
            .any(|kind| self.enabled.enabled(kind))
        {
            self.next_event_tick = None;
            return;
        }
        let seconds = match self.profile {
            ClockEventProfile::Off => None,
            ClockEventProfile::Calm => Some(self.rng.random_range(45..=75)),
            ClockEventProfile::Demo => Some(self.rng.random_range(6..=10)),
        };
        self.next_event_tick = seconds.map(|seconds| self.tick + seconds * u64::from(FIXED_HZ));
    }
}
