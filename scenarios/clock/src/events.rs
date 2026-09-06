use engine_common::ClockEventProfile;
use rand::{Rng, SeedableRng, rngs::StdRng};

pub const FIXED_HZ: u32 = 60;
pub const FALLING_TICKS: u64 = 210;
pub const REFORMING_TICKS: u64 = 90;
pub const COOLDOWN_TICKS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventPhase {
    Idle,
    Falling,
    Reforming,
    Cooldown,
}

impl EventPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Falling => "falling",
            Self::Reforming => "reforming",
            Self::Cooldown => "cooldown",
        }
    }
}

/// All event timing is measured in fixed simulation ticks, including the wait
/// before the first event. The host can pause without advancing this schedule.
#[derive(Debug)]
pub(crate) struct EventSchedule {
    pub phase: EventPhase,
    pub phase_tick: u64,
    pub tick: u64,
    pub event_id: u64,
    pub next_event_tick: Option<u64>,
    pub rng: StdRng,
    profile: ClockEventProfile,
}

impl EventSchedule {
    pub fn new(profile: ClockEventProfile, seed: u64) -> Self {
        let mut result = Self {
            phase: EventPhase::Idle,
            phase_tick: 0,
            tick: 0,
            event_id: 0,
            next_event_tick: None,
            rng: StdRng::seed_from_u64(seed),
            profile,
        };
        result.schedule_next();
        result
    }

    pub fn start(&mut self) {
        self.event_id += 1;
        self.enter(EventPhase::Falling);
        self.next_event_tick = None;
    }

    pub fn enter(&mut self, phase: EventPhase) {
        self.phase = phase;
        self.phase_tick = 0;
    }

    pub fn advance(&mut self, synchronized: bool) {
        self.tick += 1;
        self.phase_tick += 1;
        match self.phase {
            EventPhase::Idle
                if synchronized && self.next_event_tick.is_some_and(|at| self.tick >= at) =>
            {
                self.start();
            }
            EventPhase::Falling if self.phase_tick >= FALLING_TICKS => {
                self.enter(EventPhase::Reforming);
            }
            EventPhase::Reforming if self.phase_tick >= REFORMING_TICKS => {
                self.enter(EventPhase::Cooldown);
            }
            EventPhase::Cooldown if self.phase_tick >= COOLDOWN_TICKS => {
                self.enter(EventPhase::Idle);
                self.schedule_next();
            }
            _ => {}
        }
    }

    fn schedule_next(&mut self) {
        let seconds = match self.profile {
            ClockEventProfile::Off => None,
            ClockEventProfile::Calm => Some(self.rng.random_range(45..=75)),
            ClockEventProfile::Demo => Some(self.rng.random_range(6..=10)),
        };
        self.next_event_tick = seconds.map(|seconds| self.tick + seconds * u64::from(FIXED_HZ));
    }
}
