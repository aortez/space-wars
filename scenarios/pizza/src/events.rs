//! A run-local scheduler, independent of ball randomness and physics time scaling.

use engine_common::Action;
use rand::{Rng, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};

pub const FIXED_HZ: u32 = 60;
pub const DIRECTIONAL_GRAVITY_TICKS: u32 = 15 * FIXED_HZ;
pub const MIN_EVENT_GAP_TICKS: u32 = 60 * FIXED_HZ;
pub const MAX_EVENT_GAP_TICKS: u32 = 120 * FIXED_HZ;
pub const PIZZA_ACTION_VERSION: u16 = 1;
pub const PIZZA_ACTION_TRIGGER_EVENT: u32 = 1;
pub const PIZZA_OBSERVATION_VERSION: u16 = 1;

// Construct a separate stream without drawing from the existing ball RNG.
const EVENT_SEED_SALT: u64 = 0x5049_5A5A_415F_4556;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PizzaEventKind {
    DirectionalGravity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PizzaEventDirection {
    Left,
    Right,
    Top,
    Bottom,
}

impl PizzaEventDirection {
    pub const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PizzaEventTrigger {
    Automatic,
    Manual,
}

/// Tick counts use the scenario's fixed 60 Hz, before WORLD_TIME_SCALE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "kebab-case")]
pub enum PizzaEventState {
    /// Controlled sparse/dense/churn benchmarks admit no events, even manually.
    Disabled,
    Idle {
        automatic_wait_ticks: Option<u32>,
        pending_manual: Option<PizzaEventKind>,
    },
    Active {
        kind: PizzaEventKind,
        direction: PizzaEventDirection,
        trigger: PizzaEventTrigger,
        elapsed_ticks: u32,
        remaining_ticks: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PizzaObservation {
    pub version: u16,
    pub tick: u64,
    pub event: PizzaEventState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PizzaAction {
    TriggerEvent(PizzaEventKind),
}

impl PizzaAction {
    pub fn encode(self) -> Action {
        let Self::TriggerEvent(PizzaEventKind::DirectionalGravity) = self;
        let mut payload = PIZZA_ACTION_VERSION.to_le_bytes().to_vec();
        payload.push(1);
        Action::scenario(PIZZA_ACTION_TRIGGER_EVENT, payload)
    }

    pub fn decode(action: &Action) -> Option<Self> {
        let Action::Scenario { kind, payload } = action else {
            return None;
        };
        if *kind != PIZZA_ACTION_TRIGGER_EVENT
            || payload.len() != 3
            || payload[..2] != PIZZA_ACTION_VERSION.to_le_bytes()
            || payload[2] != 1
        {
            return None;
        }
        Some(Self::TriggerEvent(PizzaEventKind::DirectionalGravity))
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveEvent {
    kind: PizzaEventKind,
    direction: PizzaEventDirection,
    trigger: PizzaEventTrigger,
    elapsed_ticks: u32,
}

#[derive(Debug, Clone)]
pub(super) struct Events {
    enabled: bool,
    automatic: bool,
    wait: Option<u32>,
    pending: Option<PizzaEventKind>,
    active: Option<ActiveEvent>,
    rng: StdRng,
}

impl Events {
    pub fn new(seed: u64, automatic: bool, benchmark: bool) -> Self {
        let mut events = Self {
            enabled: !benchmark,
            automatic: automatic && !benchmark,
            wait: None,
            pending: None,
            active: None,
            rng: StdRng::seed_from_u64(seed ^ EVENT_SEED_SALT),
        };
        events.schedule_gap();
        events
    }

    pub fn state(&self) -> PizzaEventState {
        if !self.enabled {
            PizzaEventState::Disabled
        } else if let Some(active) = self.active {
            PizzaEventState::Active {
                kind: active.kind,
                direction: active.direction,
                trigger: active.trigger,
                elapsed_ticks: active.elapsed_ticks,
                remaining_ticks: DIRECTIONAL_GRAVITY_TICKS - active.elapsed_ticks,
            }
        } else {
            PizzaEventState::Idle {
                automatic_wait_ticks: self.wait,
                pending_manual: self.pending,
            }
        }
    }

    pub fn request(&mut self, kind: PizzaEventKind) -> bool {
        if !self.enabled || self.active.is_some() || self.pending.is_some() {
            return false;
        }
        self.pending = Some(kind);
        true
    }

    /// Admit before physics so all 900 active updates can use the same event.
    /// Only the fixed-step host advances time; zero-duration calls never get here.
    pub fn begin_tick(&mut self) {
        if !self.enabled || self.active.is_some() {
            return;
        }
        let (kind, trigger) = if let Some(kind) = self.pending.take() {
            (kind, PizzaEventTrigger::Manual)
        } else if self.wait == Some(0) {
            (
                PizzaEventKind::DirectionalGravity,
                PizzaEventTrigger::Automatic,
            )
        } else {
            return;
        };
        self.active = Some(ActiveEvent {
            kind,
            direction: PizzaEventDirection::ALL[self.rng.random_range(0..4)],
            trigger,
            elapsed_ticks: 0,
        });
        self.wait = None;
    }

    /// Complete the active update, or count one full update of ordinary play.
    /// The new quiet gap starts after the event's final update, not during it.
    pub fn finish_tick(&mut self) {
        if let Some(active) = &mut self.active {
            active.elapsed_ticks += 1;
            if active.elapsed_ticks == DIRECTIONAL_GRAVITY_TICKS {
                self.active = None;
                self.schedule_gap();
            }
        } else if let Some(wait) = &mut self.wait {
            *wait = wait.saturating_sub(1);
        }
    }

    fn schedule_gap(&mut self) {
        self.wait = self.automatic.then(|| {
            self.rng
                .random_range(MIN_EVENT_GAP_TICKS..=MAX_EVENT_GAP_TICKS)
        });
    }
}

#[cfg(test)]
mod tests;
