//! Session-only source allocation experiment, independent of projected plans.
use engine_common::{RenderFrame, RenderPointPool};
use std::cell::RefCell;

pub(crate) const LIMIT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SourceMode {
    #[default]
    Reference,
    Pooled,
}

impl SourceMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Reference => "reference",
            Self::Pooled => "pooled",
        }
    }
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "reference" => Ok(Self::Reference),
            "pooled" => Ok(Self::Pooled),
            _ => Err("gpu-source requires reference or pooled"),
        }
    }
}

struct State {
    mode: SourceMode,
    used: bool,
    pool: RenderPointPool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            mode: SourceMode::default(),
            used: false,
            pool: RenderPointPool::new(LIMIT_BYTES),
        }
    }
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

pub(super) fn select(mode: SourceMode) {
    STATE.with_borrow_mut(|s| {
        if s.mode != mode {
            *s = State {
                mode,
                ..State::default()
            };
        }
    });
}

#[cfg(any(test, all(target_os = "linux", feature = "pi-kiosk-gpu")))]
pub(super) fn reset() {
    STATE.with_borrow_mut(|s| *s = State::default());
}

pub(crate) fn render(
    produce: impl FnOnce(&mut RenderPointPool) -> Option<Vec<RenderFrame>>,
) -> Option<Vec<RenderFrame>> {
    STATE.with_borrow_mut(|s| {
        s.used = false;
        if s.mode != SourceMode::Pooled {
            return None;
        }
        let frames = produce(&mut s.pool);
        s.used = frames.is_some();
        if !s.used {
            s.pool = RenderPointPool::new(LIMIT_BYTES);
        }
        frames
    })
}

pub(crate) fn recycle(frames: Vec<RenderFrame>) {
    STATE.with_borrow_mut(|s| {
        if s.mode == SourceMode::Pooled && s.used {
            s.pool.recycle(frames);
        }
    });
}

#[cfg(all(target_os = "linux", feature = "pi-kiosk-gpu"))]
pub(super) fn diagnostics() -> String {
    STATE.with_borrow(|s| format!(
        "\ngpu_source_policy={}\ngpu_source_pool_applied={}\ngpu_source_pool_limit_bytes={}\ngpu_source_pool_idle_bytes={}\ngpu_source_pool_high_water_bytes={}\ngpu_source_pool_reuses={}\ngpu_source_pool_misses={}\ngpu_source_pool_discards={}",
        s.mode.name(), s.used, LIMIT_BYTES, s.pool.bytes(), s.pool.high_water_bytes, s.pool.reuses, s.pool.misses, s.pool.discards,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{RenderColor, RenderPoint, RenderPolygon, RenderPrimitive};

    #[test]
    fn reference_fallback_and_session_cleanup_do_not_retain_sources() {
        reset();
        assert_eq!(SourceMode::default(), SourceMode::Reference);
        assert!(render(|_| panic!("reference must not use pool")).is_none());
        select(SourceMode::Pooled);
        for _ in 0..2 {
            let frames = render(|p| {
                let mut f = RenderFrame::default();
                f.push_primitive(
                    0,
                    RenderPrimitive::Polygon(RenderPolygon::filled(
                        p.collect((0..4).map(|_| RenderPoint::ZERO)),
                        RenderColor::RED,
                    )),
                );
                Some(vec![f])
            })
            .unwrap();
            recycle(frames);
        }
        STATE.with_borrow(|s| assert_eq!(s.pool.reuses, 1));
        assert!(render(|_| None).is_none());
        STATE.with_borrow(|s| assert_eq!(s.pool.bytes(), 0));
        reset();
        STATE.with_borrow(|s| assert_eq!(s.mode, SourceMode::Reference));
        for m in [SourceMode::Reference, SourceMode::Pooled] {
            assert_eq!(SourceMode::parse(m.name()), Ok(m));
        }
        for value in ["", "on", "pooled\nstatus"] {
            assert!(SourceMode::parse(value).is_err());
        }
    }
}
