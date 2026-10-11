//! Optional coarse render hooks for bounded external diagnostic captures.
//! No clocks, text contents, allocations or logging live in this module.
//! With no observer, each scope only checks a thread-local function pointer.
//! Stages are inclusive and may nest; they must not be summed indiscriminately.
use std::cell::Cell;

/// A coarse CPU-side operation, not GPU execution or physical presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Stage {
    /// Text measurement through TextContext, including shaping/cache lookup.
    TextMeasure,
    /// Line-breaking query through TextContext, including shaping/cache lookup.
    TextBreak,
    /// Shaping API including cache hits, metrics cloning and glyph placement.
    TextShape,
    /// Uncached shaping run; nested within TextShape (words may still be cached).
    TextShapeMiss,
    /// Canvas text draw, including shape, masks and queuing glyph quads.
    TextDraw,
    /// CPU atlas lookup, rasterization, packing, allocation and quad construction.
    CpuMaskBuild,
    /// CPU atlas row-band uploads, including an empty loop for a warm run.
    CpuMaskUpload,
    /// Transform and queue glyph triangles; does not include their later GL draw.
    GlyphQuads,
    /// Path flattening/cache lookup, fill tessellation and command generation.
    PathFill,
    /// Path flattening/cache lookup, stroke tessellation and command generation.
    PathStroke,
    /// Font-face construction for an uncached word; requires word-detail opt-in.
    WordFace,
    /// RustyBuzz buffer setup and word shaping; requires word-detail opt-in.
    WordShape,
    /// Glyph metrics/outline discovery and result assembly, timed once per word.
    WordGlyphs,
}

/// Number of coarse stages; observers can use fixed-size storage.
pub const STAGE_COUNT: usize = 13;
/// Stable ordering and short column labels for diagnostics.
pub const STAGES: [(Stage, &str); STAGE_COUNT] = [
    (Stage::TextMeasure, "text_measure"),
    (Stage::TextBreak, "text_break"),
    (Stage::TextShape, "text_shape"),
    (Stage::TextShapeMiss, "text_shape_miss"),
    (Stage::TextDraw, "text_draw"),
    (Stage::CpuMaskBuild, "cpu_mask_build"),
    (Stage::CpuMaskUpload, "cpu_mask_upload"),
    (Stage::GlyphQuads, "glyph_quads"),
    (Stage::PathFill, "path_fill"),
    (Stage::PathStroke, "path_stroke"),
    (Stage::WordFace, "word_face"),
    (Stage::WordShape, "word_shape"),
    (Stage::WordGlyphs, "word_glyphs"),
];
/// Receives balanced begin/end notifications on the rendering thread.
/// Callbacks must not re-enter FemtoVG, panic, or retain borrowed application data.
pub type Observer = fn(Stage, bool);
thread_local! {
    static OBSERVER: Cell<Option<Observer>> = const { Cell::new(None) };
    static WORD_DETAIL: Cell<bool> = const { Cell::new(false) };
}

/// Set an observer for this thread. None is the default and disables hooks.
/// Existing scopes deliver their matching end to the observer they began with.
pub fn set_observer(observer: Option<Observer>) {
    OBSERVER.set(observer);
}

/// Enable intrusive per-word hooks for a dedicated shaping experiment.
/// Off by default; ordinary text captures keep this false. Does not install
/// an observer or enable any clock by itself. No per-glyph hooks are added.
pub fn set_word_detail(enabled: bool) {
    WORD_DETAIL.set(enabled);
}

pub(crate) struct Scope {
    stage: Stage,
    observer: Option<Observer>,
}
impl Scope {
    pub(crate) fn new(stage: Stage) -> Self {
        let observer = if matches!(stage, Stage::WordFace | Stage::WordShape | Stage::WordGlyphs) && !WORD_DETAIL.get()
        {
            None
        } else {
            OBSERVER.get()
        };
        if let Some(observer) = observer {
            observer(stage, true);
        }
        Self { stage, observer }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(observer) = self.observer {
            observer(self.stage, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    thread_local! {
        static EVENTS: RefCell<Vec<(Stage, bool)>> = const { RefCell::new(Vec::new()) };
    }
    fn observe(stage: Stage, begin: bool) {
        EVENTS.with(|events| events.borrow_mut().push((stage, begin)));
    }
    #[test]
    fn disabled_hooks_are_silent_and_existing_scopes_keep_their_observer() {
        set_observer(None);
        EVENTS.with(|events| events.borrow_mut().clear());
        drop(Scope::new(Stage::TextDraw));
        assert!(EVENTS.with(|events| events.borrow().is_empty()));
        set_observer(Some(observe));
        let outer = Scope::new(Stage::TextDraw);
        let inner = Scope::new(Stage::TextShape);
        set_observer(None);
        drop(inner);
        drop(outer);
        drop(Scope::new(Stage::PathFill));
        assert_eq!(
            EVENTS.with(|events| events.borrow().clone()),
            [
                (Stage::TextDraw, true),
                (Stage::TextShape, true),
                (Stage::TextShape, false),
                (Stage::TextDraw, false),
            ]
        );
    }

    #[test]
    fn stage_indices_and_names_are_unique_and_complete() {
        for (index, (stage, name)) in STAGES.iter().enumerate() {
            assert_eq!(*stage as usize, index);
            assert!(!name.is_empty());
            assert_eq!(STAGES.iter().filter(|(_, n)| n == name).count(), 1);
        }
    }

    #[test]
    fn word_hooks_require_separate_opt_in() {
        set_observer(Some(observe));
        set_word_detail(false);
        EVENTS.with(|events| events.borrow_mut().clear());
        drop(Scope::new(Stage::WordFace));
        assert!(EVENTS.with(|events| events.borrow().is_empty()));
        set_word_detail(true);
        drop(Scope::new(Stage::WordFace));
        set_word_detail(false);
        set_observer(None);
        assert_eq!(
            EVENTS.with(|events| events.borrow().clone()),
            [(Stage::WordFace, true), (Stage::WordFace, false)]
        );
    }

    #[test]
    fn text_errors_balance_scopes_without_changing_errors() {
        set_observer(Some(observe));
        EVENTS.with(|events| events.borrow_mut().clear());
        let text = crate::TextContext::default();
        assert!(text.measure_text(0., 0., "hello", &crate::Paint::default()).is_err());
        set_observer(None);
        let mut depth = [0usize; STAGE_COUNT];
        EVENTS.with(|events| {
            assert!(!events.borrow().is_empty());
            for &(stage, begin) in events.borrow().iter() {
                if begin {
                    depth[stage as usize] += 1;
                } else {
                    depth[stage as usize] = depth[stage as usize].checked_sub(1).unwrap();
                }
            }
        });
        assert_eq!(depth, [0; STAGE_COUNT]);
    }
}
