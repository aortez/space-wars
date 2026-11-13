//! The two lamps in the upper corners of the frame. A lamp rests at
//! `REST_BRIGHTNESS` of its full light. Each tick the lamps perceive the
//! scene, the bodies moving in view, the knocks bodies take and what
//! changed (see `perception`), and each lamp acts on that (see `brain`): it
//! notices a mover and follows it with a spotlight, searches where it lost
//! it, flares at a knock, lights the digits that change, and goes out when
//! its bulb gets too hot. The lamps draw from a seeded stream of their own,
//! never the event schedule's, and only read the scene, so they never change
//! it. The lamps' own setting decides whether they act at all: Steady lamps
//! rest.

use engine_common::{ClockLampActivity, ClockLampSide, ClockLampsState};
use engine_core::Vec2;
use rand::{SeedableRng, rngs::StdRng};

use crate::layout::Layout;

mod brain;
mod perception;
#[cfg(test)]
pub(crate) use brain::PRESENT_NARROW;
pub(crate) use brain::Stage;
pub(crate) use perception::{Facts, View};

/// At rest a lamp gives this fraction of its full light. Flickers and outages
/// dim it from here, and a spotlight brightens it past full. The plant-light
/// calibration (#634) may tune it.
pub(crate) const REST_BRIGHTNESS: f32 = 0.4;
/// A spotlight at its narrowest runs a little past full.
const SPOT_PEAK: f32 = 1.15;
/// A bulb coming back from an outage warms up from this fraction of its
/// resting brightness.
const WARM_UP_FROM: f32 = 0.55;
const WARM_UP_TICKS: u32 = 24;
pub(crate) const SPARK_TICKS: u32 = 24;

/// How a lamp looks this tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    /// The bulb's brightness: 1 is full, `REST_BRIGHTNESS` is steady and 0 is out.
    pub brightness: f32,
    /// 0 is the lamp's steady wash of light, 1 a full spotlight.
    pub narrow: f32,
    /// Where the spotlight points, in face units from the face's lower-left
    /// corner. It means nothing while `narrow` is zero.
    pub target: Vec2,
    /// The mover in the spotlight, while the lamp follows one in view.
    pub subject: Option<Subject>,
}

impl Look {
    pub const STEADY: Self = Self {
        brightness: REST_BRIGHTNESS,
        narrow: 0.0,
        target: Vec2::ZERO,
        subject: None,
    };
}

/// A mover in a spotlight, in face units like a look's target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Subject {
    /// The middle of its solid bounds.
    pub position: Vec2,
    /// Half the longer side of its solid bounds.
    pub size: f32,
}

/// Where a lamp's head hangs: just inside the junction of the frame's top
/// rail and its side rail, 0 on the left and 1 on the right.
pub(crate) fn head(lamp: usize, layout: &Layout) -> Vec2 {
    let inset = layout.frame_width * 1.3;
    let x = if lamp == 0 {
        layout.bounds_min.x + inset
    } else {
        layout.bounds_max.x - inset
    };
    Vec2::new(x, layout.bounds_max.y - inset)
}

pub(crate) struct Lamps {
    rng: StdRng,
    brain: brain::Brain,
    perception: perception::Perception,
}

impl Lamps {
    pub fn new(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed ^ 0x4c41_4d50_5321);
        let brain = brain::Brain::new(&mut rng);
        Self {
            rng,
            brain,
            perception: perception::Perception::default(),
        }
    }

    /// How `lamp` looks this tick: 0 is the left lamp, 1 the right.
    pub fn look(&self, lamp: usize) -> Look {
        self.brain.look(lamp)
    }

    pub fn spark_tick(&self, lamp: usize) -> Option<u32> {
        self.brain.spark_tick(lamp)
    }

    /// Looks at the bodies of the scene's physics worlds inside `view`, and
    /// at the scene, before the lamps step. Each world keeps its place in
    /// `worlds`, empty while it is gone.
    pub fn perceive(
        &mut self,
        worlds: &[Option<&engine_rapier::world::PhysicsWorld>],
        view: View,
        facts: Facts,
    ) {
        self.perception.attend(self.brain.attending());
        self.perception.perceive(worlds, view, facts);
    }

    /// Lets each lamp act on what the lamps perceived this tick. While
    /// `lively` is false they rest.
    pub fn step(&mut self, stage: Stage, lively: bool) {
        self.brain
            .step(&mut self.rng, self.perception.percepts(), stage, lively);
    }

    /// Starts `activity` on `lamp` now, for previews, fixtures and captures:
    /// an outage, unless the other lamp is out, a startle, a present of the
    /// digits on its side, or rest. The others need a mover.
    pub fn preview(&mut self, activity: ClockLampActivity, lamp: ClockLampSide) {
        let lamp = match lamp {
            ClockLampSide::Left => 0,
            ClockLampSide::Right => 1,
        };
        self.brain.preview(&mut self.rng, activity, lamp);
    }

    pub fn state(&self) -> ClockLampsState {
        let counts = self.brain.counts();
        ClockLampsState {
            left: self.brain.state(0),
            right: self.brain.state(1),
            notices: counts.notices,
            tracks: counts.tracks,
            handoffs: counts.handoffs,
            startles: counts.startles,
            outages: counts.outages,
            presents: counts.presents,
            fidgets: counts.fidgets,
            dips: counts.dips,
            perceived: self.perception.percepts().state(),
        }
    }
}
