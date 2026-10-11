//! CPU-only diagnostic clocks. Const-disabled callers retain the ordinary path.
use std::time::{Duration, Instant};

#[derive(Default, Debug, PartialEq)]
pub(crate) struct EncodeProfile {
    pub resolve: Duration,
    pub exact_expand: Duration,
    pub scalable_expand: Duration,
    pub fallback_fill: Duration,
    pub fallback_stroke: Duration,
    pub ordinary: Duration,
    pub validate: Duration,
    pub exact_vertices: u64,
    pub scalable_vertices: u64,
    pub scalable_ranges: u64,
    pub fallback_fills: u64,
    pub fallback_strokes: u64,
    pub ordinary_draws: u64,
    pub ordinary_geometry: DrawProfile,
    /// Cached, prepared, unsupported, merged, fine arc, nonfinite, budget.
    /// Subset of the frame's adapter attempts, not additional work.
    pub ordinary_topology: [u64; 7],
    pub guarded_stroke: DrawProfile,
    pub stroke_visibility: Duration,
    pub offscreen_stroke: Duration,
    pub offscreen_strokes: u64,
}

/// Coarse clocks around existing tessellation stages, never per-vertex clocks.
/// These are nested attribution, not extra time to add to the parent span.
#[derive(Default, Debug, PartialEq)]
pub(crate) struct DrawProfile {
    pub prepare: Duration,
    pub fill: Duration,
    pub stroke: Duration,
    pub finish: Duration,
    pub points: u64,
    pub vertices: u64,
    pub indices: u64,
    pub empty: u64,
}

impl DrawProfile {
    pub fn accumulate(&mut self, other: Self) {
        self.prepare += other.prepare;
        self.fill += other.fill;
        self.stroke += other.stroke;
        self.finish += other.finish;
        self.points += other.points;
        self.vertices += other.vertices;
        self.indices += other.indices;
        self.empty += other.empty;
    }
}

impl EncodeProfile {
    pub fn accumulate(&mut self, other: Self) {
        self.resolve += other.resolve;
        self.exact_expand += other.exact_expand;
        self.scalable_expand += other.scalable_expand;
        self.fallback_fill += other.fallback_fill;
        self.fallback_stroke += other.fallback_stroke;
        self.ordinary += other.ordinary;
        self.validate += other.validate;
        self.exact_vertices += other.exact_vertices;
        self.scalable_vertices += other.scalable_vertices;
        self.scalable_ranges += other.scalable_ranges;
        self.fallback_fills += other.fallback_fills;
        self.fallback_strokes += other.fallback_strokes;
        self.ordinary_draws += other.ordinary_draws;
        self.ordinary_geometry.accumulate(other.ordinary_geometry);
        for (total, value) in self
            .ordinary_topology
            .iter_mut()
            .zip(other.ordinary_topology)
        {
            *total += value;
        }
        self.guarded_stroke.accumulate(other.guarded_stroke);
        self.stroke_visibility += other.stroke_visibility;
        self.offscreen_stroke += other.offscreen_stroke;
        self.offscreen_strokes += other.offscreen_strokes;
    }
}

pub(super) fn started<const PROFILE: bool>() -> Option<Instant> {
    PROFILE.then(Instant::now)
}
pub(super) fn elapsed(start: Option<Instant>) -> Duration {
    start.map_or(Duration::ZERO, |start| start.elapsed())
}
