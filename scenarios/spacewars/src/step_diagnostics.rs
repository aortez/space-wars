//! Opt-in, thread-local lifecycle timings for fixed-input diagnostic replays.
//! No counters enter simulation state or run in the normal client build.
use serde::Serialize;
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

#[derive(Default, Serialize)]
pub struct Timing {
    calls: u64,
    elapsed_ns: u128,
}

pub type Timings = BTreeMap<&'static str, Timing>;

thread_local! {
    static TIMINGS: RefCell<Option<Timings>> = const { RefCell::new(None) };
}

pub fn measure<R>(operation: impl FnOnce() -> R) -> (R, Timings) {
    TIMINGS.with(|timings| {
        assert!(timings.borrow().is_none(), "step measurement cannot nest");
        *timings.borrow_mut() = Some(Timings::default());
    });
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            TIMINGS.with(|timings| *timings.borrow_mut() = None);
        }
    }
    let _reset = Reset;
    let result = operation();
    let timings = TIMINGS.with(|timings| timings.borrow_mut().take().unwrap());
    (result, timings)
}

pub(crate) struct Scope {
    name: &'static str,
    started: Option<Instant>,
}

impl Scope {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            started: TIMINGS.with(|timings| timings.borrow().is_some().then(Instant::now)),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            let elapsed_ns = started.elapsed().as_nanos();
            TIMINGS.with(|timings| {
                if let Some(timings) = timings.borrow_mut().as_mut() {
                    let timing = timings.entry(self.name).or_default();
                    timing.calls += 1;
                    timing.elapsed_ns += elapsed_ns;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_are_inert_outside_measurement_and_count_each_call() {
        drop(Scope::new("inactive"));
        let (result, timings) = measure(|| {
            let _outer = Scope::new("outer");
            drop(Scope::new("inner"));
            drop(Scope::new("inner"));
            42
        });
        assert_eq!(result, 42);
        assert_eq!(timings.len(), 2);
        assert_eq!(timings["outer"].calls, 1);
        assert_eq!(timings["inner"].calls, 2);
        assert!(measure(|| ()).1.is_empty());
    }

    #[test]
    fn panic_clears_the_measurement_without_poisoning_the_next_one() {
        assert!(std::panic::catch_unwind(|| measure(|| panic!("probe"))).is_err());
        assert!(measure(|| ()).1.is_empty());
    }

    #[test]
    fn nested_measurement_is_rejected_without_losing_outer_scopes() {
        let (_, timings) = measure(|| {
            assert!(std::panic::catch_unwind(|| measure(|| ())).is_err());
            drop(Scope::new("after-rejected-nesting"));
        });
        assert_eq!(timings["after-rejected-nesting"].calls, 1);
    }
}
