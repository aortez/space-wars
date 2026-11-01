//! What Clock publishes about its digit surfaces, the lit digit cells a
//! resident can attach to: a sample of the surfaces alive now, the latest
//! changes to them, what has happened to them over the run and the bounds
//! they keep within.

use serde::{Deserialize, Serialize};

/// What is moving a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockSurfaceSource {
    /// At rest on its place in the face.
    Anchored,
    /// Digit Slide's authored slide through its slot.
    Slide,
    /// A rigid body in the event's canonical world, named by the surface.
    Rigid,
    /// Standalone Meltdown's ballistic step, which has no rigid world.
    Ballistic,
    /// An authored path back to the face after physical motion.
    Return,
    /// Meltdown's refill, which grows each new cell upward where it belongs.
    Refill,
    /// Cell Swap's exchange paths and its correcting fade.
    Swap,
    /// A Slot Machine reel.
    Reel,
    /// A Marquee carrying the face with its own transform.
    Marquee,
}

impl ClockSurfaceSource {
    pub const ALL: [Self; 9] = [
        Self::Anchored,
        Self::Slide,
        Self::Rigid,
        Self::Ballistic,
        Self::Return,
        Self::Refill,
        Self::Swap,
        Self::Reel,
        Self::Marquee,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anchored => "anchored",
            Self::Slide => "slide",
            Self::Rigid => "rigid",
            Self::Ballistic => "ballistic",
            Self::Return => "return",
            Self::Refill => "refill",
            Self::Swap => "swap",
            Self::Reel => "reel",
            Self::Marquee => "marquee",
        }
    }
}

/// Why a surface stopped existing. Support is lost with it: nothing attached
/// to a retired surface moves to another one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockSurfaceRetirement {
    /// Its glyph gave way to another: a new reading, font or format, or the
    /// glyph an event brings in.
    Replaced,
    /// The event moving it ended early, away from its place in the face.
    Interrupted,
    /// A surplus Explosion block that shrank into its neighbour.
    MergedAway,
    /// Melted into water on contact with the floor.
    Melted,
    /// Left the view as a solid.
    Exited,
    /// Solid material Meltdown reclaimed when the face refilled.
    Reclaimed,
}

impl ClockSurfaceRetirement {
    pub const ALL: [Self; 6] = [
        Self::Replaced,
        Self::Interrupted,
        Self::MergedAway,
        Self::Melted,
        Self::Exited,
        Self::Reclaimed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Replaced => "replaced",
            Self::Interrupted => "interrupted",
            Self::MergedAway => "merged-away",
            Self::Melted => "melted",
            Self::Exited => "exited",
            Self::Reclaimed => "reclaimed",
        }
    }
}

/// One live surface as the last step left it. Positions and lengths are in
/// world mill units. Its frame maps a local point `p`, in grid units, to
/// `origin + rotate(p * scale, angle)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockDigitSurface {
    /// The serial it was admitted with, which a run never reuses.
    pub id: u32,
    /// The digit slot it was admitted in.
    pub slot: u8,
    /// The grid column and row it was admitted at.
    pub cell: [i8; 2],
    pub source: ClockSurfaceSource,
    /// The physics entity of the rigid body carrying it, while one does.
    pub body: Option<u64>,
    pub origin_milli: [i32; 2],
    pub angle_millidegrees: i32,
    /// World mill units per grid unit.
    pub scale_milli: i32,
    /// The supporting rectangle in its own frame, in thousandths of a grid
    /// unit: min x, min y, max x, max y. Empty supports nothing.
    pub footprint_milli: [i32; 4],
    /// Carries what is already attached to it until it is retired, and takes
    /// nothing new.
    pub departing: bool,
    /// Presentation only: how much of its ink is drawn, 0..=1000.
    pub opacity_milli: u16,
    /// Presentation only: the world band of y it is drawn within.
    pub clip_milli: Option<[i32; 2]>,
}

/// How many live surfaces one source is moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSurfaceSourceCount {
    pub source: ClockSurfaceSource,
    pub live: usize,
}

/// One lifecycle change. A surface is admitted once, handed off between
/// sources any number of times, departs at most once and is retired once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ClockSurfaceChangeKind {
    Admitted {
        slot: u8,
        cell: [i8; 2],
        source: ClockSurfaceSource,
    },
    HandedOff {
        from: ClockSurfaceSource,
        to: ClockSurfaceSource,
    },
    Departing,
    Retired {
        reason: ClockSurfaceRetirement,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSurfaceChange {
    /// Counts every change the run has recorded, so a gap shows that changes
    /// were missed.
    pub sequence: u64,
    pub tick: u64,
    pub id: u32,
    pub change: ClockSurfaceChangeKind,
}

/// Surfaces retired over the run, by reason.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSurfaceRetirements {
    pub replaced: u64,
    pub interrupted: u64,
    pub merged_away: u64,
    pub melted: u64,
    pub exited: u64,
    pub reclaimed: u64,
}

impl ClockSurfaceRetirements {
    pub const fn get(&self, reason: ClockSurfaceRetirement) -> u64 {
        match reason {
            ClockSurfaceRetirement::Replaced => self.replaced,
            ClockSurfaceRetirement::Interrupted => self.interrupted,
            ClockSurfaceRetirement::MergedAway => self.merged_away,
            ClockSurfaceRetirement::Melted => self.melted,
            ClockSurfaceRetirement::Exited => self.exited,
            ClockSurfaceRetirement::Reclaimed => self.reclaimed,
        }
    }

    pub const fn total(&self) -> u64 {
        self.replaced
            + self.interrupted
            + self.merged_away
            + self.melted
            + self.exited
            + self.reclaimed
    }
}

/// What the surfaces are now, and what has happened to them over the run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSurfaceAccounting {
    /// Surfaces alive now.
    pub live: usize,
    /// Those of them departing.
    pub departing: usize,
    /// The most alive at once over the run.
    pub peak_live: usize,
    pub admitted: u64,
    pub handed_off: u64,
    pub departed: u64,
    pub retired: ClockSurfaceRetirements,
    /// The last change's sequence number.
    pub sequence: u64,
}

/// The bounds the surfaces and these diagnostics keep within.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSurfaceLimits {
    /// The most surfaces alive at once.
    pub live: usize,
    /// The changes Clock retains for consumers that read less often than
    /// every step.
    pub retained_changes: usize,
    /// The most surfaces in `sample`.
    pub sample: usize,
    /// The most changes in `recent`.
    pub recent: usize,
}

/// Clock's digit surfaces: a bounded sample of those alive now, the latest
/// changes to them and their accounting over the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockDigitSurfaceState {
    pub accounting: ClockSurfaceAccounting,
    /// The live surfaces by what is moving them, each source moving any, in
    /// the order of `ClockSurfaceSource::ALL`.
    pub sources: Vec<ClockSurfaceSourceCount>,
    /// The surface with the lowest identity in each slot for each source,
    /// departing or not, by slot, then staying before departing, then source.
    /// At most `limits.sample` of them.
    pub sample: Vec<ClockDigitSurface>,
    /// The latest `limits.recent` changes at most, oldest first.
    pub recent: Vec<ClockSurfaceChange>,
    pub limits: ClockSurfaceLimits,
}
