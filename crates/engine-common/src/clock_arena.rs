//! What Clock publishes about its shared rigid arena: who holds it, what its
//! world holds, the bounds it takes claims within and why it refuses one.

use serde::{Deserialize, Serialize};

use crate::ClockEventKind;

/// How many bodies, colliders and joints there are, or may be.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockArenaPopulation {
    pub bodies: usize,
    pub colliders: usize,
    pub joints: usize,
}

impl ClockArenaPopulation {
    pub const NONE: Self = Self::new(0, 0);

    /// Bodies and colliders without joints.
    pub const fn new(bodies: usize, colliders: usize) -> Self {
        Self {
            bodies,
            colliders,
            joints: 0,
        }
    }

    pub const fn plus(self, other: Self) -> Self {
        Self {
            bodies: self.bodies + other.bodies,
            colliders: self.colliders + other.colliders,
            joints: self.joints + other.joints,
        }
    }

    /// The larger of each count.
    pub const fn max(self, other: Self) -> Self {
        const fn larger(a: usize, b: usize) -> usize {
            if a > b { a } else { b }
        }
        Self {
            bodies: larger(self.bodies, other.bodies),
            colliders: larger(self.colliders, other.colliders),
            joints: larger(self.joints, other.joints),
        }
    }

    pub const fn within(self, limit: Self) -> bool {
        self.bodies <= limit.bodies
            && self.colliders <= limit.colliders
            && self.joints <= limit.joints
    }
}

/// Who holds the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ClockArenaOwner {
    /// A visit's terrain and character, or the terrain its character left
    /// while an event held the arena.
    Visit,
    /// An event leasing the visit's arena.
    Event { event: ClockEventKind },
    /// Retained soil, a resident.
    Soil,
    /// One of the arena fixture's owners, a resident.
    Fixture { index: u8 },
}

/// Why the arena could not take a claim or start an event. Nothing changed:
/// the arena, its owners and their bodies are as they were.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockArenaUnavailable {
    /// The arena already holds as many residents as it may.
    Residents,
    /// The owners' budgets together would be more than the arena holds.
    Capacity,
    /// The resting residents have not fitted the view since a resize, so
    /// the arena takes no visit or event that uses the floor.
    LayoutIncompatible,
}

impl ClockArenaUnavailable {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Residents => "residents",
            Self::Capacity => "capacity",
            Self::LayoutIncompatible => "layout-incompatible",
        }
    }

    /// What a person asking for the claim is told.
    pub const fn notice(self) -> &'static str {
        match self {
            Self::Residents => "The arena holds all the residents it can",
            Self::Capacity => "The arena has no room for this",
            Self::LayoutIncompatible => "The arena's residents do not fit this view",
        }
    }
}

/// What a resize did to the arena's resting residents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ClockArenaRemap {
    /// Every resting resident lies within the new view, so none moved.
    Exact,
    /// `moved` resting residents lay beyond the new view's edges and came in
    /// just far enough to lie within them.
    Clamped { moved: usize },
    /// A resting resident could not come in without landing in another
    /// resident or in the floor, or is wider than the view. Nothing moved.
    Declined,
}

/// An owner holding the arena: what it has in the arena's world, and the most
/// it may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockArenaClaim {
    pub owner: ClockArenaOwner,
    /// A resident keeps the arena for the rest of the run.
    pub resident: bool,
    pub population: ClockArenaPopulation,
    pub budget: ClockArenaPopulation,
}

/// The arena's last rigid step. Timings are in microseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockArenaStep {
    pub wall_us: u64,
    pub collision_detection_us: u64,
    pub solver_us: u64,
    pub ccd_substeps: usize,
    pub active_bodies: usize,
    pub sleeping_bodies: usize,
    pub candidate_pairs: usize,
    pub contact_pairs: usize,
    pub contacts: usize,
}

/// The bounds the arena takes claims within.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockArenaLimits {
    /// The most owners that hold the arena at once.
    pub claims: usize,
    /// The most residents among them, soil included.
    pub residents: usize,
    /// The most bodies, colliders and joints the arena holds at once.
    pub capacity: ClockArenaPopulation,
}

/// What the arena holds now, and what it has done over the run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockArenaAccounting {
    /// Owners holding the arena now.
    pub claims: usize,
    /// Those of them that are residents.
    pub residents: usize,
    /// Everything in the arena's world.
    pub bodies: usize,
    pub colliders: usize,
    pub joints: usize,
    /// Claims granted over the run.
    pub granted: u64,
    /// Claims let go over the run.
    pub released: u64,
    /// Previews, player joins and fixtures the arena turned away over the
    /// run. Events the schedule leaves out, and `clock trigger` requests the
    /// client fails, never reach it.
    pub refused: u64,
    /// Rigid steps over the run. Exactly one per positive tick while a world
    /// exists.
    pub steps: u64,
    /// Worlds the arena has held over the run.
    pub worlds: u64,
    /// The resting residents have not fitted the view since a resize. The
    /// arena keeps the floor they last fitted and takes no visit or event
    /// that uses the floor.
    pub layout_incompatible: bool,
    /// What the last resize did to the resting residents.
    pub last_remap: Option<ClockArenaRemap>,
}

/// Clock's shared rigid arena: who holds it, what each holder has in its
/// world, its last step and the bounds it takes claims within.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockArenaState {
    pub accounting: ClockArenaAccounting,
    /// The owners holding the arena, in the order they claimed it.
    pub claims: Vec<ClockArenaClaim>,
    /// The floor the arena keeps under resting residents while no visit's
    /// terrain is one.
    pub floor: ClockArenaPopulation,
    /// The arena's last step, unless it has not stepped yet.
    pub last_step: Option<ClockArenaStep>,
    pub limits: ClockArenaLimits,
}
