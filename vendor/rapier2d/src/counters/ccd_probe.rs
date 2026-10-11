//! Opt-in instrumentation for the retained Spacewars CCD experiment.
use std::{cell::RefCell, time::Instant};

#[derive(Default, Debug, Clone, Copy)]
#[cfg_attr(feature = "serde-serialize", derive(serde::Serialize))]
/// Diagnostic-only inclusive CCD prefilter totals on the current thread.
pub struct Totals {
    /// Collider pairs considered for a time of impact.
    pub toi_calls: u64,
    /// Inclusive nanoseconds in time-of-impact processing.
    pub toi_ns: u128,
    /// Thickness evaluations, including pairs rejected before a shape cast.
    pub thickness_calls: u64,
    /// Nanoseconds evaluating shape thickness.
    pub thickness_ns: u128,
    /// Pairs rejected by the existing relative-motion/thickness check.
    pub thickness_rejected: u64,
    /// Compound cache requests; misses are included.
    pub compound_lookups: u64,
    /// Compound properties evaluated on a cache miss (step-local in the pipeline).
    pub compound_misses: u64,
    /// Total immediate compound children visited on cache misses.
    pub compound_children: u64,
    /// Time computing compound thickness on cache misses.
    pub compound_compute_ns: u128,
    /// Uncached primitive thickness calls.
    pub primitive_thickness_calls: u64,
    /// Uncached primitive thickness calculation time.
    pub primitive_thickness_ns: u128,
    /// First-impact setup time; bounds and BVH updates nest within this.
    pub first_setup_ns: u128,
    /// First-impact search time, including TOI calls and swept bounds.
    pub first_search_ns: u128,
    /// Motion-clamping setup time, including bounds and BVH updates.
    pub clamp_setup_ns: u128,
    /// Motion-clamping initial search time, including TOI calls and bounds.
    pub clamp_search_ns: u128,
    /// Follow-up resweeps and pseudo-intersection handling after impacts.
    pub resweep_ns: u128,
    /// Swept bounds calculation time, nested within setup/search/resweeps.
    pub bounds_ns: u128,
    /// Number of swept bounds calculations.
    pub bounds_calls: u64,
    /// BVH update time, nested within setup.
    pub bvh_update_ns: u128,
    /// Number of BVH updates.
    pub bvh_update_calls: u64,
    /// Candidate iterator traversal time, nested within searches/resweeps.
    pub candidate_next_ns: u128,
    /// Iterator next calls, including the final exhausted call.
    pub candidate_next_calls: u64,
    /// Existing-contact lookup/deepest-contact time before each TOI check.
    pub contact_lookup_ns: u128,
    /// Number of existing-contact lookups.
    pub contact_lookup_calls: u64,
    /// First-impact AABB candidates, before self/duplicate/group filtering.
    pub first_candidates: u64,
    /// Initial motion-clamping AABB candidates, before filtering.
    pub clamp_candidates: u64,
    /// Resweep AABB candidates, before filtering.
    pub resweep_candidates: u64,
}

pub(crate) enum Phase {
    FirstSetup,
    FirstSearch,
    ClampSetup,
    ClampSearch,
    Resweep,
    Bounds,
    BvhUpdate,
    CandidateNext,
    ContactLookup,
    CompoundThickness,
    PrimitiveThickness,
}

pub(crate) struct PhaseSpan {
    started: Option<Instant>,
    phase: Phase,
}

impl PhaseSpan {
    pub(crate) fn new(phase: Phase) -> Self {
        Self {
            started: TOTALS.with(|totals| totals.borrow().is_some().then(Instant::now)),
            phase,
        }
    }
}

impl Drop for PhaseSpan {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let ns = started.elapsed().as_nanos();
            TOTALS.with(|totals| {
                if let Some(totals) = totals.borrow_mut().as_mut() {
                    match self.phase {
                        Phase::FirstSetup => totals.first_setup_ns += ns,
                        Phase::FirstSearch => totals.first_search_ns += ns,
                        Phase::ClampSetup => totals.clamp_setup_ns += ns,
                        Phase::ClampSearch => totals.clamp_search_ns += ns,
                        Phase::Resweep => totals.resweep_ns += ns,
                        Phase::Bounds => {
                            totals.bounds_ns += ns;
                            totals.bounds_calls += 1;
                        }
                        Phase::BvhUpdate => {
                            totals.bvh_update_ns += ns;
                            totals.bvh_update_calls += 1;
                        }
                        Phase::CandidateNext => {
                            totals.candidate_next_ns += ns;
                            totals.candidate_next_calls += 1;
                        }
                        Phase::ContactLookup => {
                            totals.contact_lookup_ns += ns;
                            totals.contact_lookup_calls += 1;
                        }
                        Phase::CompoundThickness => {
                            totals.compound_compute_ns += ns;
                        }
                        Phase::PrimitiveThickness => {
                            totals.primitive_thickness_ns += ns;
                            totals.primitive_thickness_calls += 1;
                        }
                    }
                }
            });
        }
    }
}

pub(crate) fn compound_lookup() {
    TOTALS.with(|totals| {
        if let Some(totals) = totals.borrow_mut().as_mut() {
            totals.compound_lookups += 1;
        }
    });
}

pub(crate) fn compound_miss(parts: usize) {
    TOTALS.with(|totals| {
        if let Some(totals) = totals.borrow_mut().as_mut() {
            totals.compound_misses += 1;
            totals.compound_children += parts as u64;
        }
    });
}

pub(crate) fn timed_candidates<I: Iterator>(mut iter: I) -> impl Iterator<Item = I::Item> {
    std::iter::from_fn(move || {
        let _span = PhaseSpan::new(Phase::CandidateNext);
        iter.next()
    })
}

pub(crate) fn candidate(phase: Phase) {
    TOTALS.with(|totals| {
        if let Some(totals) = totals.borrow_mut().as_mut() {
            match phase {
                Phase::FirstSearch => totals.first_candidates += 1,
                Phase::ClampSearch => totals.clamp_candidates += 1,
                Phase::Resweep => totals.resweep_candidates += 1,
                _ => unreachable!(),
            }
        }
    });
}

std::thread_local! {
    static TOTALS: RefCell<Option<Totals>> = const { RefCell::new(None) };
}

/// Start one non-nesting synchronous measurement.
pub fn begin() {
    TOTALS.with(|totals| {
        assert!(totals.borrow().is_none());
        *totals.borrow_mut() = Some(Totals::default());
    });
}

/// Stop the measurement and take its counters, or return empty counters.
pub fn finish() -> Totals {
    TOTALS.with(|totals| totals.borrow_mut().take().unwrap_or_default())
}

pub(crate) struct Span {
    started: Option<Instant>,
    thickness: bool,
}

impl Span {
    pub(crate) fn new(thickness: bool) -> Self {
        Self {
            started: TOTALS.with(|totals| totals.borrow().is_some().then(Instant::now)),
            thickness,
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            TOTALS.with(|totals| {
                if let Some(totals) = totals.borrow_mut().as_mut() {
                    if self.thickness {
                        totals.thickness_calls += 1;
                        totals.thickness_ns += started.elapsed().as_nanos();
                    } else {
                        totals.toi_calls += 1;
                        totals.toi_ns += started.elapsed().as_nanos();
                    }
                }
            });
        }
    }
}

pub(crate) fn rejected() {
    TOTALS.with(|totals| {
        if let Some(totals) = totals.borrow_mut().as_mut() {
            totals.thickness_rejected += 1;
        }
    });
}
