//! Fixed-size per-measurement render diagnostics; disabled in timing controls.
//! CPU/wall spans are inclusive. There are no per-glyph/word/vertex clocks.
use femtovg::diagnostics::{self, STAGE_COUNT, STAGES, Stage};
use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct Stamp {
    wall: Instant,
    cpu: Option<Duration>,
}
impl Stamp {
    fn now() -> Self {
        Self {
            wall: Instant::now(),
            cpu: super::cpu().ok(),
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Active {
    depth: usize,
    start: Option<Stamp>,
}
#[derive(Clone, Copy, Default)]
struct Sample {
    calls: u64,
    wall_ms: f64,
    cpu_ms: Option<f64>,
}
#[derive(Default)]
pub(super) struct Totals {
    enabled: bool,
    samples: [Sample; STAGE_COUNT],
}
#[derive(Default)]
struct Profile {
    totals: Totals,
    active: [Active; STAGE_COUNT],
}
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static PROFILE: RefCell<Option<Profile>> = const { RefCell::new(None) };
}
pub(super) fn enable(enabled: bool) {
    ENABLED.set(enabled);
}
pub(super) struct Capture {
    active: bool,
}
impl Capture {
    pub(super) fn begin() -> Self {
        let active = ENABLED.get();
        if active {
            PROFILE.with(|p| {
                *p.borrow_mut() = Some(Profile {
                    totals: Totals {
                        enabled: true,
                        ..Default::default()
                    },
                    ..Default::default()
                })
            });
            diagnostics::set_observer(Some(observe));
        }
        Self { active }
    }
    pub(super) fn finish(mut self) -> Totals {
        if self.active {
            diagnostics::set_observer(None);
            self.active = false;
            PROFILE.with(|p| p.borrow_mut().take().unwrap().totals)
        } else {
            Totals::default()
        }
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        if self.active {
            diagnostics::set_observer(None);
            PROFILE.with(|p| *p.borrow_mut() = None);
        }
    }
}
fn observe(stage: Stage, begin: bool) {
    PROFILE.with(|p| {
        let mut p = p.borrow_mut();
        let Some(p) = p.as_mut() else { return };
        let active = &mut p.active[stage as usize];
        if begin {
            if active.depth == 0 {
                active.start = Some(Stamp::now());
            }
            active.depth += 1;
        } else if active.depth > 0 {
            active.depth -= 1;
            if active.depth != 0 {
                return;
            }
            let Some(start) = active.start.take() else {
                return;
            };
            let end = Stamp::now();
            let cpu = start
                .cpu
                .zip(end.cpu)
                .and_then(|(s, e)| e.checked_sub(s))
                .map(|d| d.as_secs_f64() * 1000.);
            let sample = &mut p.totals.samples[stage as usize];
            sample.cpu_ms = if sample.calls == 0 {
                cpu
            } else {
                sample.cpu_ms.zip(cpu).map(|(a, b)| a + b)
            };
            sample.wall_ms += end.wall.duration_since(start.wall).as_secs_f64() * 1000.;
            sample.calls += 1;
        }
    });
}
pub(super) fn header() {
    print!(",text_detail");
    for (_, name) in STAGES {
        print!(",femto_{name}_calls,femto_{name}_wall_ms,femto_{name}_cpu_ms");
    }
    println!();
}
impl Totals {
    pub(super) fn report(&self) {
        print!(",{}", u8::from(self.enabled));
        for sample in &self.samples {
            print!(",{},{:.6},", sample.calls, sample.wall_ms);
            if let Some(cpu) = sample.cpu_ms {
                print!("{cpu:.6}");
            }
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_is_opt_in_and_finishing_or_dropping_clears_it() {
        enable(false);
        let c = Capture::begin();
        observe(Stage::TextShape, true);
        observe(Stage::TextShape, false);
        assert!(!c.finish().enabled);
        enable(true);
        let c = Capture::begin();
        observe(Stage::TextShape, true);
        observe(Stage::TextShape, true);
        observe(Stage::TextShape, false);
        observe(Stage::TextShape, false);
        let result = c.finish();
        assert!(result.enabled);
        assert_eq!(result.samples[Stage::TextShape as usize].calls, 1);
        assert_eq!(result.samples[Stage::TextDraw as usize].cpu_ms, None);
        assert!(PROFILE.with(|p| p.borrow().is_none()));
        drop(Capture::begin());
        assert!(PROFILE.with(|p| p.borrow().is_none()));
        enable(false);
    }
}
