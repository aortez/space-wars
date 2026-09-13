//! Opt-in, thread-local timings of one read-only observation. These scopes are
//! absent from normal builds and never enter simulation state or observations.
use serde::Serialize;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Default, Serialize)]
pub struct Stage {
    pub calls: usize,
    pub inclusive_ms: f64,
    pub exclusive_ms: f64,
}

#[derive(Default, Serialize)]
pub struct Profile {
    pub stages: BTreeMap<&'static str, Stage>,
    pub counters: BTreeMap<&'static str, u64>,
    #[serde(skip)]
    children: Vec<Duration>,
}

/// Accumulate loop work locally, then touch the shared profile once on drop.
/// In particular, a capsule sample does not perform a map lookup or clock read.
pub(super) struct Counter {
    name: &'static str,
    value: Cell<u64>,
}

impl Counter {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            value: Cell::new(0),
        }
    }

    pub fn add(&self, amount: usize) {
        self.value.set(self.value.get() + amount as u64);
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        ACTIVE.with(|p| {
            if let Some(p) = p.borrow_mut().as_mut() {
                *p.counters.entry(self.name).or_default() += self.value.get();
            }
        });
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Profile>> = const { RefCell::new(None) };
}

pub fn measure<T>(operation: impl FnOnce() -> T) -> (T, Profile) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|p| *p.borrow_mut() = None);
        }
    }
    ACTIVE.with(|p| {
        assert!(p.borrow().is_none(), "sensor profiles cannot nest");
        *p.borrow_mut() = Some(Profile::default());
    });
    let _reset = Reset;
    let value = operation();
    let profile = ACTIVE.with(|p| p.borrow_mut().take().unwrap());
    assert!(profile.children.is_empty());
    (value, profile)
}

pub(super) struct Scope {
    name: &'static str,
    start: Option<Instant>,
}

impl Scope {
    pub fn new(name: &'static str) -> Self {
        let start = ACTIVE.with(|p| {
            let mut p = p.borrow_mut();
            p.as_mut().map(|p| {
                p.children.push(Duration::ZERO);
                Instant::now()
            })
        });
        Self { name, start }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let Some(start) = self.start else {
            return;
        };
        let elapsed = start.elapsed();
        ACTIVE.with(|p| {
            let mut p = p.borrow_mut();
            let p = p.as_mut().unwrap();
            let children = p.children.pop().unwrap();
            if let Some(parent) = p.children.last_mut() {
                *parent += elapsed;
            }
            let stage = p.stages.entry(self.name).or_default();
            stage.calls += 1;
            stage.inclusive_ms += elapsed.as_secs_f64() * 1000.0;
            stage.exclusive_ms += elapsed.saturating_sub(children).as_secs_f64() * 1000.0;
        });
    }
}
