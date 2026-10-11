//! Borrowed font faces live for one shaping run only. The original per-word
//! construction remains the default/control. Fallback order is unchanged.
use super::{font::Font, FontId};
use crate::ErrorKind;
use slotmap::{DefaultKey, SlotMap};

const FACE_LIMIT: usize = 8;

pub(super) struct RunFonts<'a> {
    fonts: &'a SlotMap<DefaultKey, Font>,
    reuse: bool,
    faces: Vec<(FontId, rustybuzz::Face<'a>)>,
}
impl<'a> RunFonts<'a> {
    pub(super) fn new(fonts: &'a SlotMap<DefaultKey, Font>, reuse: bool) -> Self {
        Self {
            fonts,
            reuse,
            faces: Vec::new(),
        }
    }

    fn attempt<F, T>(&mut self, id: FontId, font: &'a Font, callback: &mut F) -> (bool, T)
    where
        F: FnMut((FontId, &Font, &rustybuzz::Face<'_>)) -> (bool, T),
    {
        if self.reuse {
            if let Some((_, face)) = self.faces.iter().find(|(cached, _)| *cached == id) {
                return callback((id, font, face));
            }
            if self.faces.len() < FACE_LIMIT {
                let face = parse_face(font);
                self.faces.push((id, face));
                return callback((id, font, &self.faces.last().unwrap().1));
            }
        }
        // More than eight fallback fonts still work; only reuse is limited.
        let face = parse_face(font);
        callback((id, font, &face))
    }

    pub(super) fn find<F, T>(&mut self, ids: &[Option<FontId>; 8], mut callback: F) -> Result<T, ErrorKind>
    where
        F: FnMut((FontId, &Font, &rustybuzz::Face<'_>)) -> (bool, T),
    {
        let fonts = self.fonts;
        // Keep the upstream order, including retried paint fonts and .notdef.
        for id in ids {
            if let Some(id) = *id {
                if let Some(font) = fonts.get(id.0) {
                    let (missing, result) = self.attempt(id, font, &mut callback);
                    if !missing {
                        return Ok(result);
                    }
                }
            } else {
                break;
            }
        }
        for (id, font) in fonts {
            let (missing, result) = self.attempt(FontId(id), font, &mut callback);
            if !missing {
                return Ok(result);
            }
        }
        if let Some((id, font)) = fonts.iter().next() {
            return Ok(self.attempt(FontId(id), font, &mut callback).1);
        }
        Err(ErrorKind::NoFontFound)
    }
}

fn parse_face(font: &Font) -> rustybuzz::Face<'_> {
    #[cfg(feature = "render-diagnostics")]
    let _scope = crate::diagnostics::Scope::new(crate::diagnostics::Stage::WordFace);
    font.face_ref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextContextImpl;

    const SANS: &[u8] = include_bytes!("../../../../scenarios/clock/fonts/DejaVuSans.ttf");

    #[test]
    fn fallback_attempts_match_upstream_including_missing_ids_and_notdef() {
        let mut context = TextContextImpl::default();
        let ids: Vec<_> = (0..10).map(|_| context.add_font_mem(SANS).unwrap()).collect();
        context.fonts.remove(ids[1].0);
        for selected in [
            [None; 8],
            [
                Some(ids[1]),
                Some(ids[3]),
                Some(ids[2]),
                None,
                Some(ids[4]),
                None,
                None,
                None,
            ],
            std::array::from_fn(|i| Some(ids[9 - i])),
        ] {
            for accepted in [Some(ids[3]), Some(ids[9]), None] {
                let mut expected = Vec::new();
                let result = context
                    .find_font(&selected, |(id, _)| {
                        expected.push(id);
                        (Some(id) != accepted, id)
                    })
                    .unwrap();
                for reuse in [false, true] {
                    let mut fonts = RunFonts::new(&context.fonts, reuse);
                    for _ in 0..2 {
                        let mut actual = Vec::new();
                        let found = fonts
                            .find(&selected, |(id, _, _)| {
                                actual.push(id);
                                (Some(id) != accepted, id)
                            })
                            .unwrap();
                        assert_eq!(actual, expected);
                        assert_eq!(found, result);
                        assert!(fonts.faces.len() <= FACE_LIMIT);
                        if !reuse {
                            assert!(fonts.faces.is_empty());
                        }
                    }
                }
            }
        }
        let mut fonts = RunFonts::new(&context.fonts, true);
        fonts.find(&[None; 8], |(id, _, _)| (true, id)).unwrap();
        assert_eq!(fonts.faces.len(), FACE_LIMIT);
        let empty = SlotMap::with_key();
        assert!(matches!(
            RunFonts::new(&empty, true).find(&[None; 8], |_| (true, ())),
            Err(ErrorKind::NoFontFound)
        ));
    }

    #[cfg(feature = "cpu-glyph-cache")]
    #[test]
    fn shaped_metrics_are_identical_across_modes_widths_fonts_and_directions() {
        use crate::{request_glyph_cache, GlyphCacheMode, Paint, TextContext};
        let mut reference = None;
        for mode in [GlyphCacheMode::CpuRunFaces, GlyphCacheMode::CpuShapeFaces] {
            request_glyph_cache(mode);
            let context = TextContext::default();
            let sans = context.add_font_mem(SANS).unwrap();
            let serif = context
                .add_font_mem(include_bytes!("../../../../scenarios/clock/fonts/DejaVuSerif.ttf"))
                .unwrap();
            let mut output = String::new();
            for font in [vec![sans], vec![serif], vec![serif, sans], vec![]] {
                for size in [12., 24., 25.] {
                    let mut paint = Paint::default().with_font(&font).with_font_size(size);
                    paint.set_letter_spacing(0.25);
                    for text in [
                        "",
                        "First result: Player 2 wins!",
                        "wordbreakinglongword",
                        "  a b\t c\nnext",
                        "caf\u{e9} e\u{301}",
                        "abc שלום مرحبا 123",
                        "missing \u{10ffff} glyph",
                    ] {
                        for width in [1., 16., 120., 560.] {
                            let metrics = context.measure_text(13., 17., text, &paint).unwrap();
                            let end = context.break_text(width, text, &paint).unwrap();
                            assert!(text.is_char_boundary(end));
                            output.push_str(&format!("{metrics:?}/{end};"));
                            let warm = context.measure_text(13., 17., text, &paint).unwrap();
                            assert_eq!(format!("{metrics:?}"), format!("{warm:?}"));
                        }
                    }
                }
            }
            if let Some(reference) = &reference {
                assert_eq!(&output, reference);
            }
            reference = Some(output);
        }
        request_glyph_cache(GlyphCacheMode::Vector);
    }

    #[cfg(all(feature = "cpu-glyph-cache", feature = "render-diagnostics"))]
    #[test]
    fn reuse_parses_once_per_uncached_run_and_never_on_cache_hits() {
        use crate::{diagnostics, request_glyph_cache, GlyphCacheMode, Paint, TextContext};
        use std::cell::Cell;
        thread_local! { static PARSES: Cell<usize> = const { Cell::new(0) }; }
        fn observe(stage: diagnostics::Stage, begin: bool) {
            if begin && stage == diagnostics::Stage::WordFace {
                PARSES.set(PARSES.get() + 1);
            }
        }
        diagnostics::set_word_detail(true);
        diagnostics::set_observer(Some(observe));
        for mode in [GlyphCacheMode::CpuRunFaces, GlyphCacheMode::CpuShapeFaces] {
            request_glyph_cache(mode);
            let context = TextContext::default();
            let font = context.add_font_mem(SANS).unwrap();
            let paint = Paint::default().with_font(&[font]).with_font_size(24.);
            PARSES.set(0);
            context.measure_text(0., 0., "one two three", &paint).unwrap();
            let first = PARSES.get();
            if mode == GlyphCacheMode::CpuShapeFaces {
                assert_eq!(first, 1);
            } else {
                assert!(first > 1);
            }
            context.measure_text(0., 0., "one two three", &paint).unwrap();
            assert_eq!(PARSES.get(), first);
            context.measure_text(0., 0., "four five six", &paint).unwrap();
            if mode == GlyphCacheMode::CpuShapeFaces {
                assert_eq!(PARSES.get(), 2);
            } else {
                assert!(PARSES.get() > first + 1);
            }
        }
        diagnostics::set_observer(None);
        diagnostics::set_word_detail(false);
        request_glyph_cache(GlyphCacheMode::Vector);
    }
}
