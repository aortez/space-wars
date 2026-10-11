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
