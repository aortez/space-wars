//! Opt-in, thread-local wall timings for development runners. Spans are
//! inclusive: for example, packing is part of settling, not additional work.
//! Disabled spans do not read a clock. These diagnostics are never state inputs.
use std::{cell::RefCell, collections::BTreeMap, marker::PhantomData, rc::Rc, time::Instant};

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Sample {
    pub calls: u64,
    pub total_ms: f64,
    pub max_ms: f64,
}

#[derive(Default)]
struct Profile {
    enabled: bool,
    samples: BTreeMap<&'static str, Sample>,
}

thread_local! {
    static PROFILE: RefCell<Profile> = RefCell::default();
}

/// Enable or disable collection on this thread, clearing earlier samples.
/// Call only between spans (normally before a benchmark's first tick).
pub fn set_enabled(enabled: bool) {
    PROFILE.with_borrow_mut(|profile| {
        profile.enabled = enabled;
        profile.samples.clear();
    });
}

/// Drain completed spans outside the timed update, usually once per tick.
pub fn take() -> BTreeMap<&'static str, Sample> {
    PROFILE.with_borrow_mut(|profile| std::mem::take(&mut profile.samples))
}

#[must_use]
pub(crate) struct Span {
    name: &'static str,
    started: Option<Instant>,
    // A span must finish on the thread whose collector it started in.
    thread: PhantomData<Rc<()>>,
}

pub(crate) fn span(name: &'static str) -> Span {
    Span {
        name,
        started: PROFILE.with_borrow(|profile| profile.enabled.then(Instant::now)),
        thread: PhantomData,
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            PROFILE.with_borrow_mut(|profile| {
                let sample = profile.samples.entry(self.name).or_default();
                sample.calls += 1;
                sample.total_ms += elapsed;
                sample.max_ms = sample.max_ms.max(elapsed);
            });
        }
    }
}
