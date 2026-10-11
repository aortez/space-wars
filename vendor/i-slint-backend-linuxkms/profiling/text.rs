// Copyright © Space-Wars contributors
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

//! Fixed-storage, opt-in text/path clocks. No per-glyph, word or vertex timers.
use super::{Stage, Stamp, PROFILE};
use femtovg::diagnostics::{self, Stage as D, STAGE_COUNT};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, Default)]
struct Active {
    start: Option<Stamp>,
    depth: usize,
}
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static ACTIVE: RefCell<[Active; STAGE_COUNT]> = RefCell::new([Active::default(); STAGE_COUNT]);
}
pub(super) fn enable(enabled: bool) {
    if ENABLED.replace(enabled) == enabled {
        return;
    }
    ACTIVE.with(|active| *active.borrow_mut() = [Active::default(); STAGE_COUNT]);
    diagnostics::set_observer(enabled.then_some(observe as _));
}
fn stage(stage: D) -> Option<Stage> {
    Some(match stage {
        D::TextMeasure => Stage::FemtoTextMeasure,
        D::TextBreak => Stage::FemtoTextBreak,
        D::TextShape => Stage::FemtoTextShape,
        D::TextShapeMiss => Stage::FemtoTextShapeMiss,
        D::TextDraw => Stage::FemtoTextDraw,
        D::CpuMaskBuild => Stage::FemtoCpuMaskBuild,
        D::CpuMaskUpload => Stage::FemtoCpuMaskUpload,
        D::GlyphQuads => Stage::FemtoGlyphQuads,
        D::PathFill => Stage::FemtoPathFill,
        D::PathStroke => Stage::FemtoPathStroke,
        // Intrusive word-level hooks belong to the isolated shaping lab, not
        // ordinary live text traces, even if another caller enables them.
        D::WordFace | D::WordShape | D::WordGlyphs => return None,
    })
}
fn observe(operation: D, begin: bool) {
    let Some(stage) = stage(operation) else {
        return;
    };
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let active = &mut active[operation as usize];
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
            let elapsed = start.elapsed();
            PROFILE.with(|p| {
                let mut p = p.borrow_mut();
                if p.trace.text_recording() {
                    p.record(stage, elapsed);
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::super::{activate_femtovg, render_trace, Scope};
    use super::*;

    #[test]
    fn text_clocks_only_record_armed_loops_and_stop_at_capture_limit() {
        enable(false);
        activate_femtovg((640, 480), None, Some(8));
        render_trace("start-text").unwrap();
        assert!(!ENABLED.get());
        for _ in 0..240 {
            let _loop = Scope::new(Stage::Loop);
            assert!(ENABLED.get());
            observe(D::TextShape, true);
            observe(D::TextShape, true);
            observe(D::TextShape, false);
            observe(D::TextShape, false);
        }
        assert!(!ENABLED.get());
        PROFILE.with(|p| {
            let p = p.borrow();
            assert!(!p.trace.recording());
            assert_eq!(
                p.samples.back().unwrap().stages[Stage::FemtoTextShape as usize].calls,
                1
            );
        });
        render_trace("start").unwrap();
        {
            let _loop = Scope::new(Stage::Loop);
            assert!(!ENABLED.get());
            let text = femtovg::TextContext::default();
            let _ = text.measure_text(0., 0., "ignored", &femtovg::Paint::default());
        }
        PROFILE.with(|p| {
            assert_eq!(
                p.borrow().samples.back().unwrap().stages[Stage::FemtoTextShape as usize].calls,
                0
            );
        });
        render_trace("cancel").unwrap();
    }

    #[test]
    fn cancelling_drops_inflight_detail_and_does_not_leak_into_next_capture() {
        enable(false);
        activate_femtovg((640, 480), None, Some(8));
        render_trace("start-text").unwrap();
        {
            let _loop = Scope::new(Stage::Loop);
            observe(D::TextDraw, true);
            render_trace("cancel").unwrap();
            observe(D::TextDraw, false);
            assert!(!ENABLED.get());
        }
        render_trace("start-text").unwrap();
        {
            let _loop = Scope::new(Stage::Loop);
            observe(D::TextDraw, true);
            observe(D::TextDraw, false);
        }
        PROFILE.with(|p| {
            assert_eq!(
                p.borrow().samples.back().unwrap().stages[Stage::FemtoTextDraw as usize].calls,
                1
            )
        });
        render_trace("cancel").unwrap();
    }

    #[test]
    fn live_collector_never_times_word_level_hooks() {
        for operation in [D::WordFace, D::WordShape, D::WordGlyphs] {
            assert!(stage(operation).is_none());
            observe(operation, true);
            observe(operation, false);
            ACTIVE.with(|active| assert_eq!(active.borrow()[operation as usize].depth, 0));
        }
    }
}
