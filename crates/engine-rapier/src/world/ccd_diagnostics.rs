//! Opt-in query counts for a synchronous, single-threaded CCD experiment.
//! Measurements stay outside simulation state and are absent in normal builds.
use rapier2d::parry::shape::Shape;
use serde::Serialize;
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

#[derive(Default, Serialize)]
pub struct QueryCounts {
    queries: u64,
    nodes: u64,
    rejected_nodes: u64,
    leaves: u64,
    primitive_queries: u64,
    max_parts: usize,
    elapsed_ns: u128,
}

#[derive(Default, Serialize)]
pub struct CcdProfile {
    pairs: BTreeMap<String, QueryCounts>,
    prefilter: rapier2d::counters::ccd_probe::Totals,
}

#[derive(Default)]
struct Frame {
    depth: usize,
    key: String,
    current: QueryCounts,
    profile: CcdProfile,
}

thread_local! {
    static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) };
}

/// Counts only queries inside this closure on the current thread. Inclusive
/// query times contain instrumentation overhead and are not benchmark results.
pub fn measure<R>(operation: impl FnOnce() -> R) -> (R, CcdProfile) {
    FRAME.with(|frame| {
        assert!(frame.borrow().is_none(), "CCD measurement cannot nest");
        *frame.borrow_mut() = Some(Frame::default());
    });
    rapier2d::counters::ccd_probe::begin();
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FRAME.with(|frame| *frame.borrow_mut() = None);
            rapier2d::counters::ccd_probe::finish();
        }
    }
    let reset = Reset;
    let result = operation();
    let mut profile = FRAME.with(|frame| frame.borrow_mut().take().unwrap().profile);
    profile.prefilter = rapier2d::counters::ccd_probe::finish();
    drop(reset);
    (result, profile)
}

pub(super) struct Scope {
    enabled: bool,
    started: Option<Instant>,
}

impl Scope {
    pub(super) fn enter(a: &dyn Shape, b: &dyn Shape) -> Self {
        FRAME.with(|frame| {
            let mut frame = frame.borrow_mut();
            let Some(frame) = frame.as_mut() else {
                return Self {
                    enabled: false,
                    started: None,
                };
            };
            let started = (frame.depth == 0).then(Instant::now);
            if frame.depth == 0 {
                let mut kinds = [
                    format!("{:?}", a.shape_type()),
                    format!("{:?}", b.shape_type()),
                ];
                kinds.sort_unstable();
                frame.key = kinds.join("/");
                frame.current = QueryCounts {
                    queries: 1,
                    max_parts: [a, b]
                        .into_iter()
                        .filter_map(|shape| shape.as_compound())
                        .map(|shape| shape.shapes().len())
                        .max()
                        .unwrap_or(0),
                    ..Default::default()
                };
            }
            if a.as_compound().is_none() && b.as_compound().is_none() {
                frame.current.primitive_queries += 1;
            }
            frame.depth += 1;
            Self {
                enabled: true,
                started,
            }
        })
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        FRAME.with(|frame| {
            let mut frame = frame.borrow_mut();
            let frame = frame.as_mut().unwrap();
            frame.depth -= 1;
            if let Some(started) = self.started {
                let current = &frame.current;
                let total = frame.profile.pairs.entry(frame.key.clone()).or_default();
                total.queries += current.queries;
                total.nodes += current.nodes;
                total.rejected_nodes += current.rejected_nodes;
                total.leaves += current.leaves;
                total.primitive_queries += current.primitive_queries;
                total.max_parts = total.max_parts.max(current.max_parts);
                total.elapsed_ns += started.elapsed().as_nanos();
            }
        });
    }
}

pub(super) fn node(rejected: bool) {
    FRAME.with(|frame| {
        if let Some(frame) = frame.borrow_mut().as_mut() {
            frame.current.nodes += 1;
            frame.current.rejected_nodes += u64::from(rejected);
        }
    });
}

pub(super) fn leaf() {
    FRAME.with(|frame| {
        if let Some(frame) = frame.borrow_mut().as_mut() {
            frame.current.leaves += 1;
        }
    });
}
