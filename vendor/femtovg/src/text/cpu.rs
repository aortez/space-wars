//! Space-Wars experimental fill-mask path. See SPACEWARS_PATCH.md.
//! CPU atlas storage is bounded; unsupported runs use the existing frame-local
//! vector/bitmap atlas. Never overwrite or delete a page with queued draws.
use super::{FontId, RenderMode, ShapedGlyph, TextMetrics};
use crate::{
    Atlas, Canvas, DrawCommand, ErrorKind, GlyphDrawCommands, ImageFlags, ImageId, ImageSource, PixelFormat, Quad,
    Renderer,
};
use ab_glyph_rasterizer::{point, Point, Rasterizer};
use fnv::FnvHashMap;
use rgb::RGBA8;
use rustybuzz::ttf_parser::{GlyphId, OutlineBuilder};
use std::cell::Cell;

const SIDE: usize = 512;
const PAGE_LIMIT: usize = 4;
const ENTRY_LIMIT: usize = 4096;

/// Experimental glyph-mask policy. Vector is the unchanged default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlyphCacheMode {
    /// Generate glyph masks using the upstream vector renderer.
    #[default]
    Vector,
    /// Generate eligible fill masks on the CPU, with bounded retained storage.
    Cpu,
    /// Same CPU masks and uploads, reusing each font face within a text run.
    CpuRunFaces,
    /// CPU run faces plus bounded face reuse across uncached words in a shaping run.
    CpuShapeFaces,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Request {
    mode: GlyphCacheMode,
    generation: u64,
}

/// Thread-local diagnostics across live canvases; totals are monotonic.
/// Page/entry counts are current CPU-atlas residency, not total GPU memory.
#[derive(Clone, Copy, Debug, Default)]
pub struct GlyphCacheStats {
    /// Requested policy, applied at each canvas's next safe text-frame boundary.
    pub requested: GlyphCacheMode,
    /// Canvases currently holding a CPU cache, including an empty cache.
    pub cpu_canvases: usize,
    /// Resident CPU pages, each with 512x512 RGBA CPU pixels and a GPU texture.
    pub pages: usize,
    /// Resident mask entries.
    pub entries: usize,
    /// Number of masks rasterized since thread startup.
    pub rasterized: u64,
    /// Font faces parsed by CPU mask generation, excluding upstream shaping.
    pub face_parses: u64,
    /// Number of CPU-to-GPU row-band uploads.
    pub uploads: u64,
    /// Logical uploaded bytes, not physical bus traffic.
    pub uploaded_bytes: u64,
    /// Runs using the original atlas while CPU mode was active.
    pub fallback_runs: u64,
    /// Safe cache resets due to policy requests or capacity pressure.
    pub resets: u64,
    /// CPU-path errors; a failed cache falls back until explicitly reset.
    pub errors: u64,
}
thread_local! {
    static REQUEST: Cell<Request> = const { Cell::new(Request { mode: GlyphCacheMode::Vector, generation: 0 }) };
    static STATS: Cell<GlyphCacheStats> = Cell::new(GlyphCacheStats::default());
}
fn stats(f: impl FnOnce(&mut GlyphCacheStats)) {
    let mut value = STATS.get();
    f(&mut value);
    STATS.set(value);
}
/// Request a policy and a fresh glyph cache for canvases on this thread.
/// Does not mutate persisted settings or interrupt a queued frame. Even a
/// repeated request resets the cache, useful for controlled cold comparisons.
pub fn request_glyph_cache(mode: GlyphCacheMode) {
    let previous = REQUEST.get();
    REQUEST.set(Request {
        mode,
        generation: previous.generation.wrapping_add(1),
    });
    stats(|s| s.requested = mode);
}
/// Read CPU glyph-cache diagnostics on the rendering thread.
pub fn glyph_cache_stats() -> GlyphCacheStats {
    STATS.get()
}

pub(crate) fn reuse_shaping_faces() -> bool {
    REQUEST.get().mode == GlyphCacheMode::CpuShapeFaces
}

#[derive(Default)]
pub(crate) struct CacheState {
    request: Request,
    pub(crate) drawn: bool,
    atlas: Option<CpuAtlas>,
}
impl CacheState {
    pub(crate) fn is_cpu(&self) -> bool {
        self.request.mode != GlyphCacheMode::Vector
    }
    pub(crate) fn render<T: Renderer>(
        &mut self,
        canvas: &mut Canvas<T>,
        layout: &TextMetrics,
        size: f32,
        mode: RenderMode,
    ) -> Option<GlyphDrawCommands> {
        let atlas = self.atlas.as_mut()?;
        let result = if mode == RenderMode::Fill {
            atlas.render(
                canvas,
                layout,
                size,
                matches!(
                    self.request.mode,
                    GlyphCacheMode::CpuRunFaces | GlyphCacheMode::CpuShapeFaces
                ),
            )
        } else {
            Ok(None)
        };
        match result {
            Ok(Some(commands)) => Some(commands),
            result => {
                stats(|s| s.fallback_runs += 1);
                if let Err(error) = result {
                    atlas.failed = true;
                    stats(|s| s.errors += 1);
                    log::warn!("CPU glyph cache failed; using vector fallback until reset: {error:?}");
                }
                None
            }
        }
    }
}
impl<T: Renderer> Canvas<T> {
    pub(crate) fn prepare_glyph_cache(&mut self) {
        let mut state = std::mem::take(&mut self.cpu_glyphs);
        let requested = REQUEST.get();
        if !state.drawn && (state.request != requested || state.atlas.as_ref().is_some_and(|a| a.recycle)) {
            // Previous commands were submitted by flush. Before the first text
            // of this frame, no queued command can reference these old pages.
            if let Some(mut atlas) = state.atlas.take() {
                atlas.clear(self);
            }
            let vector = self.glyph_atlas.clone();
            vector.clear(self);
            state.request = requested;
            state.atlas = (requested.mode != GlyphCacheMode::Vector).then(CpuAtlas::new);
            stats(|s| s.resets += 1);
        }
        state.drawn = true;
        self.cpu_glyphs = state;
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct Key {
    font: FontId,
    glyph: u16,
    size: u32,
    subpixel: u8,
}
#[derive(Clone, Copy)]
struct Entry {
    page: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    bearing_y: f32,
}
struct Page {
    packing: Atlas,
    pixels: Vec<RGBA8>,
    image: ImageId,
    dirty: Option<(usize, usize)>,
}
struct CpuAtlas {
    pages: Vec<Page>,
    entries: FnvHashMap<Key, Entry>,
    recycle: bool,
    failed: bool,
    page_limit: usize,
    entry_limit: usize,
}
impl CpuAtlas {
    fn new() -> Self {
        stats(|s| s.cpu_canvases += 1);
        Self {
            pages: Vec::new(),
            entries: FnvHashMap::default(),
            recycle: false,
            failed: false,
            page_limit: PAGE_LIMIT,
            entry_limit: ENTRY_LIMIT,
        }
    }
    fn clear<T: Renderer>(&mut self, canvas: &mut Canvas<T>) {
        // Keep lengths until Drop, which updates the residency diagnostics.
        for page in &self.pages {
            canvas.delete_image(page.image);
        }
    }
    fn allocate<T: Renderer>(
        &mut self,
        canvas: &mut Canvas<T>,
        w: usize,
        h: usize,
    ) -> Result<Option<(usize, usize, usize)>, ErrorKind> {
        for (index, page) in self.pages.iter_mut().enumerate() {
            if let Some((x, y)) = page.packing.add_rect(w, h) {
                return Ok(Some((index, x, y)));
            }
        }
        if self.pages.len() == self.page_limit {
            self.recycle = true;
            return Ok(None);
        }
        let mut packing = Atlas::new(SIDE, SIDE);
        let Some((x, y)) = packing.add_rect(w, h) else {
            return Ok(None);
        };
        let image = canvas.create_image_empty(SIDE, SIDE, PixelFormat::Rgba8, ImageFlags::NEAREST)?;
        self.pages.push(Page {
            packing,
            pixels: vec![RGBA8::new(0, 0, 0, 255); SIDE * SIDE],
            image,
            dirty: None,
        });
        stats(|s| s.pages += 1);
        Ok(Some((self.pages.len() - 1, x, y)))
    }
    fn render<T: Renderer>(
        &mut self,
        canvas: &mut Canvas<T>,
        layout: &TextMetrics,
        size: f32,
        reuse_faces: bool,
    ) -> Result<Option<GlyphDrawCommands>, ErrorKind> {
        #[cfg(feature = "render-diagnostics")]
        let build = crate::diagnostics::Scope::new(crate::diagnostics::Stage::CpuMaskBuild);
        if self.failed || self.recycle || !size.is_finite() || !(1.0..=92.0).contains(&size) {
            return Ok(None);
        }
        if layout.glyphs.iter().any(|g| !eligible(g)) {
            return Ok(None);
        }
        let text_context = canvas.text_context.clone();
        let text_context = text_context.borrow();
        let mut commands = Vec::<DrawCommand>::new();
        // The faces borrow font bytes owned by this immutable text-context
        // borrow. Keep them only for this run; no persistent font-data copies,
        // self-references, invalidation policy, or changes to upstream shaping.
        // An all-hit run neither parses a face nor allocates this map.
        let mut run_faces = FnvHashMap::default();
        for glyph in layout.glyphs.iter().filter(|g| !g.c.is_control()) {
            let fraction = crate::geometry::quantize(glyph.x.fract(), 0.1);
            let key = Key {
                font: glyph.font_id,
                glyph: glyph.codepoint as u16,
                size: size.to_bits(),
                subpixel: (fraction * 10.) as u8,
            };
            let entry = if let Some(entry) = self.entries.get(&key) {
                *entry
            } else {
                if self.entries.len() == self.entry_limit {
                    self.recycle = true;
                    return Ok(None);
                }
                let font = text_context.font(glyph.font_id).ok_or(ErrorKind::NoFontFound)?;
                let parse = || {
                    stats(|s| s.face_parses += 1);
                    font.face_ref()
                };
                let uncached_face;
                let face: &rustybuzz::Face<'_> = if reuse_faces {
                    run_faces.entry(glyph.font_id).or_insert_with(parse)
                } else {
                    uncached_face = parse();
                    &uncached_face
                };
                if face.is_variable() {
                    return Ok(None);
                }
                let w = glyph.width.ceil() as usize + 4;
                let h = glyph.height.ceil() as usize + 4;
                let mut outline = Outline::new(
                    w,
                    h,
                    font.scale(size),
                    2. - glyph.bearing_x + fraction,
                    2. + glyph.bearing_y.round(),
                );
                if face.outline_glyph(GlyphId(key.glyph), &mut outline).is_none()
                    && (glyph.width > 0. || glyph.height > 0.)
                {
                    return Ok(None);
                }
                let Some((page, x, y)) = self.allocate(canvas, w, h)? else {
                    return Ok(None);
                };
                let target = &mut self.pages[page];
                outline.raster.for_each_pixel(|i, coverage| {
                    let a = (coverage.clamp(0., 1.) * 255.).round() as u8;
                    target.pixels[(y + i / w) * SIDE + x + i % w] = RGBA8::new(a, a, a, 255);
                });
                target.dirty = Some(match target.dirty {
                    Some((top, bottom)) => (top.min(y), bottom.max(y + h)),
                    None => (y, y + h),
                });
                let entry = Entry {
                    page,
                    x: x + 1,
                    y: y + 1,
                    width: w - 2,
                    height: h - 2,
                    bearing_y: glyph.bearing_y.round(),
                };
                self.entries.insert(key, entry);
                stats(|s| {
                    s.entries += 1;
                    s.rasterized += 1;
                });
                entry
            };
            let image_id = self.pages[entry.page].image;
            let index = commands.iter().position(|c| c.image_id == image_id).unwrap_or_else(|| {
                commands.push(DrawCommand {
                    image_id,
                    quads: Vec::new(),
                });
                commands.len() - 1
            });
            let x = glyph.x.trunc() - 1.;
            let y = (glyph.y + glyph.bearing_y).round() - entry.bearing_y - 1.;
            commands[index].quads.push(Quad {
                x0: x,
                y0: y,
                x1: x + entry.width as f32,
                y1: y + entry.height as f32,
                s0: entry.x as f32 / SIDE as f32,
                t0: entry.y as f32 / SIDE as f32,
                s1: (entry.x + entry.width) as f32 / SIDE as f32,
                t1: (entry.y + entry.height) as f32 / SIDE as f32,
            });
        }
        // Batch once per dirty page per text run, before queuing its quads. This
        // preserves synchronous upload error handling in the existing API.
        #[cfg(feature = "render-diagnostics")]
        drop(build);
        #[cfg(feature = "render-diagnostics")]
        let _upload = crate::diagnostics::Scope::new(crate::diagnostics::Stage::CpuMaskUpload);
        for page in &mut self.pages {
            if let Some((top, bottom)) = page.dirty.take() {
                let data = imgref::ImgRef::new(&page.pixels[top * SIDE..bottom * SIDE], SIDE, bottom - top);
                canvas.update_image(page.image, ImageSource::Rgba(data), 0, top)?;
                stats(|s| {
                    s.uploads += 1;
                    s.uploaded_bytes += (SIDE * (bottom - top) * 4) as u64;
                });
            }
        }
        Ok(Some(GlyphDrawCommands {
            alpha_glyphs: commands,
            color_glyphs: Vec::new(),
        }))
    }
}
impl Drop for CpuAtlas {
    fn drop(&mut self) {
        stats(|s| {
            s.cpu_canvases -= 1;
            s.pages -= self.pages.len();
            s.entries -= self.entries.len();
        });
    }
}
fn eligible(g: &ShapedGlyph) -> bool {
    !g.bitmap_glyph
        && g.codepoint <= u16::MAX as u32
        && g.x.is_finite()
        && g.x >= 0.
        && g.y.is_finite()
        && g.bearing_x.is_finite()
        && g.bearing_y.is_finite()
        && g.width.is_finite()
        && g.height.is_finite()
        && (0.0..=(SIDE - 4) as f32).contains(&g.width)
        && (0.0..=(SIDE - 4) as f32).contains(&g.height)
}

struct Outline {
    raster: Rasterizer,
    scale: f32,
    offset: Point,
    last: Point,
    first: Point,
}
impl Outline {
    fn new(w: usize, h: usize, scale: f32, x: f32, y: f32) -> Self {
        Self {
            raster: Rasterizer::new(w, h),
            scale,
            offset: point(x, y),
            last: point(0., 0.),
            first: point(0., 0.),
        }
    }
    fn at(&self, x: f32, y: f32) -> Point {
        point(self.offset.x + x * self.scale, self.offset.y - y * self.scale)
    }
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.last = self.at(x, y);
        self.first = self.last;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let next = self.at(x, y);
        self.raster.draw_line(self.last, next);
        self.last = next;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let next = self.at(x, y);
        self.raster.draw_quad(self.last, self.at(x1, y1), next);
        self.last = next;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let next = self.at(x, y);
        self.raster
            .draw_cubic(self.last, self.at(x1, y1), self.at(x2, y2), next);
        self.last = next;
    }
    fn close(&mut self) {
        self.raster.draw_line(self.last, self.first);
        self.last = self.first;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        paint::GlyphTexture, renderer::Command, renderer::Vertex, Color, DummyImage, ImageInfo, ImageStore, Paint,
        RecordingRenderer,
    };

    #[derive(Default)]
    struct CheckedRenderer {
        inner: RecordingRenderer,
        fail_upload: bool,
    }
    impl Renderer for CheckedRenderer {
        type Image = DummyImage;
        type NativeTexture = ();
        type Surface = ();
        type CommandBuffer = ();
        fn set_size(&mut self, w: u32, h: u32, dpi: f32) {
            self.inner.set_size(w, h, dpi);
        }
        fn render(
            &mut self,
            surface: &(),
            images: &mut ImageStore<DummyImage>,
            verts: &[Vertex],
            commands: Vec<Command>,
        ) {
            for command in &commands {
                let glyph = match command.glyph_texture {
                    GlyphTexture::AlphaMask(id) | GlyphTexture::ColorTexture(id) => Some(id),
                    _ => None,
                };
                for image in glyph.into_iter().chain(command.image) {
                    assert!(images.get(image).is_some(), "queued draw references a deleted image");
                }
            }
            self.inner.render(surface, images, verts, commands);
        }
        fn alloc_image(&mut self, info: ImageInfo) -> Result<DummyImage, ErrorKind> {
            self.inner.alloc_image(info)
        }
        fn create_image_from_native_texture(&mut self, texture: (), info: ImageInfo) -> Result<DummyImage, ErrorKind> {
            self.inner.create_image_from_native_texture(texture, info)
        }
        fn update_image(
            &mut self,
            image: &mut DummyImage,
            data: ImageSource,
            x: usize,
            y: usize,
        ) -> Result<(), ErrorKind> {
            if std::mem::take(&mut self.fail_upload) {
                return Err(ErrorKind::ImageUpdateOutOfBounds);
            }
            self.inner.update_image(image, data, x, y)
        }
        fn delete_image(&mut self, image: DummyImage, id: ImageId) {
            self.inner.delete_image(image, id);
        }
        fn screenshot(&mut self) -> Result<imgref::ImgVec<RGBA8>, ErrorKind> {
            self.inner.screenshot()
        }
    }
    fn canvas(mode: GlyphCacheMode) -> (Canvas<CheckedRenderer>, Paint) {
        assert_eq!(glyph_cache_stats().cpu_canvases, 0);
        REQUEST.set(Request::default());
        STATS.set(GlyphCacheStats::default());
        if mode != GlyphCacheMode::Vector {
            request_glyph_cache(mode);
        }
        let mut canvas = Canvas::new(CheckedRenderer::default()).unwrap();
        canvas.set_size(800, 480, 1.);
        let font = canvas
            .add_font_mem(include_bytes!("../../../../scenarios/clock/fonts/DejaVuSans.ttf"))
            .unwrap();
        let paint = Paint::color(Color::white()).with_font(&[font]).with_font_size(24.);
        (canvas, paint)
    }
    fn draw(canvas: &mut Canvas<CheckedRenderer>, paint: &Paint, text: &str) {
        canvas.fill_text(32., 64., text, paint).unwrap();
    }
    fn assert_no_residency() {
        let s = glyph_cache_stats();
        assert_eq!((s.cpu_canvases, s.pages, s.entries), (0, 0, 0));
    }

    #[test]
    fn vector_is_the_unchanged_default() {
        let (mut canvas, paint) = canvas(GlyphCacheMode::Vector);
        draw(&mut canvas, &paint, "Round Complete");
        canvas.flush_to_surface(&());
        assert_eq!(glyph_cache_stats().requested, GlyphCacheMode::Vector);
        assert_eq!(glyph_cache_stats().rasterized, 0);
        assert_no_residency();
    }

    #[test]
    fn warm_text_has_no_cpu_raster_or_upload_work_and_drop_releases_residency() {
        let (mut canvas, paint) = canvas(GlyphCacheMode::Cpu);
        draw(&mut canvas, &paint, "Round Complete");
        canvas.flush_to_surface(&());
        let cold = glyph_cache_stats();
        assert!(cold.rasterized > 0);
        assert_eq!((cold.pages, cold.uploads, cold.fallback_runs), (1, 1, 0));
        draw(&mut canvas, &paint, "Round Complete");
        canvas.flush_to_surface(&());
        let warm = glyph_cache_stats();
        assert_eq!(
            (warm.rasterized, warm.uploads, warm.entries),
            (cold.rasterized, cold.uploads, cold.entries)
        );
        drop(canvas);
        assert_no_residency();
    }

    #[test]
    fn switching_and_explicit_resets_wait_until_queued_text_is_submitted() {
        let (mut canvas, paint) = canvas(GlyphCacheMode::Cpu);
        draw(&mut canvas, &paint, "A");
        let old = canvas.cpu_glyphs.atlas.as_ref().unwrap().pages[0].image;
        request_glyph_cache(GlyphCacheMode::Vector);
        draw(&mut canvas, &paint, "B");
        assert!(canvas.cpu_glyphs.is_cpu());
        assert!(canvas.images.get(old).is_some());
        canvas.flush_to_surface(&()); // CheckedRenderer verifies both draws still have textures.
        draw(&mut canvas, &paint, "C");
        assert!(!canvas.cpu_glyphs.is_cpu());
        assert!(canvas.images.get(old).is_none());
        assert_no_residency();
        request_glyph_cache(GlyphCacheMode::Cpu);
        draw(&mut canvas, &paint, "D");
        assert!(!canvas.cpu_glyphs.is_cpu());
        canvas.flush_to_surface(&());
        draw(&mut canvas, &paint, "A");
        let old = canvas.cpu_glyphs.atlas.as_ref().unwrap().pages[0].image;
        request_glyph_cache(GlyphCacheMode::Cpu);
        draw(&mut canvas, &paint, "A");
        assert!(canvas.images.get(old).is_some());
        canvas.flush_to_surface(&());
        draw(&mut canvas, &paint, "A");
        assert!(canvas.images.get(old).is_none());
        assert_eq!(glyph_cache_stats().pages, 1);
    }

    #[test]
    fn entry_limit_falls_back_and_recycles_only_at_a_frame_boundary() {
        let (mut canvas, paint) = canvas(GlyphCacheMode::Cpu);
        canvas.prepare_glyph_cache();
        canvas.cpu_glyphs.atlas.as_mut().unwrap().entry_limit = 1;
        draw(&mut canvas, &paint, "A");
        let old = canvas.cpu_glyphs.atlas.as_ref().unwrap().pages[0].image;
        draw(&mut canvas, &paint, "B");
        assert_eq!(glyph_cache_stats().entries, 1);
        assert_eq!(glyph_cache_stats().fallback_runs, 1);
        assert!(canvas.images.get(old).is_some());
        canvas.flush_to_surface(&());
        assert!(canvas.ephemeral_glyph_atlas.is_none());
        draw(&mut canvas, &paint, "B");
        assert!(canvas.images.get(old).is_none());
        assert_eq!((glyph_cache_stats().entries, glyph_cache_stats().resets), (1, 2));
    }

    #[test]
    fn page_limit_is_bounded_and_recycling_deletes_old_textures() {
        let (mut canvas, _) = canvas(GlyphCacheMode::Cpu);
        canvas.prepare_glyph_cache();
        let mut state = std::mem::take(&mut canvas.cpu_glyphs);
        let atlas = state.atlas.as_mut().unwrap();
        for _ in 0..PAGE_LIMIT {
            // The upstream skyline packer reserves its bottom boundary.
            assert!(atlas.allocate(&mut canvas, SIDE, SIDE - 1).unwrap().is_some());
        }
        assert!(atlas.allocate(&mut canvas, 2, 2).unwrap().is_none());
        assert_eq!(glyph_cache_stats().pages, PAGE_LIMIT);
        let old = atlas.pages.iter().map(|p| p.image).collect::<Vec<_>>();
        canvas.cpu_glyphs = state;
        canvas.prepare_glyph_cache();
        assert_eq!(glyph_cache_stats().pages, PAGE_LIMIT);
        canvas.flush_to_surface(&());
        canvas.prepare_glyph_cache();
        assert_eq!(glyph_cache_stats().pages, 0);
        assert!(old.iter().all(|id| canvas.images.get(*id).is_none()));
    }

    #[test]
    fn unsupported_runs_use_a_frame_local_fallback() {
        let (mut canvas, paint) = canvas(GlyphCacheMode::Cpu);
        canvas.stroke_text(32., 64., "Outlined", &paint).unwrap();
        canvas.fill_text(-20., 80., "Left clipped", &paint).unwrap();
        assert_eq!(glyph_cache_stats().fallback_runs, 2);
        assert_eq!(glyph_cache_stats().pages, 0);
        assert!(canvas.ephemeral_glyph_atlas.is_some());
        canvas.flush_to_surface(&());
        assert!(canvas.ephemeral_glyph_atlas.is_none());
        let metrics = canvas.measure_text(32., 64., "A", &paint).unwrap();
        let glyph = metrics.glyphs[0];
        assert!(eligible(&glyph));
        for modified in [
            ShapedGlyph {
                bitmap_glyph: true,
                ..glyph
            },
            ShapedGlyph { x: f32::NAN, ..glyph },
            ShapedGlyph {
                codepoint: 65536,
                ..glyph
            },
            ShapedGlyph { width: 513., ..glyph },
            ShapedGlyph { height: -1., ..glyph },
        ] {
            assert!(!eligible(&modified));
        }
    }

    #[test]
    fn upload_failure_falls_back_until_explicit_reset() {
        let (mut canvas, paint) = canvas(GlyphCacheMode::Cpu);
        canvas.renderer.fail_upload = true;
        draw(&mut canvas, &paint, "A");
        assert_eq!((glyph_cache_stats().errors, glyph_cache_stats().fallback_runs), (1, 1));
        canvas.flush_to_surface(&());
        draw(&mut canvas, &paint, "B");
        assert_eq!((glyph_cache_stats().errors, glyph_cache_stats().fallback_runs), (1, 2));
        canvas.flush_to_surface(&());
        request_glyph_cache(GlyphCacheMode::Cpu);
        draw(&mut canvas, &paint, "B");
        assert_eq!(glyph_cache_stats().uploads, 1);
        assert_eq!(glyph_cache_stats().fallback_runs, 2);
        canvas.flush_to_surface(&());
        drop(canvas);
        assert_no_residency();
    }

    #[test]
    fn run_faces_parse_once_per_font_and_preserve_mixed_font_pixels() {
        fn mixed(mode: GlyphCacheMode) -> Vec<RGBA8> {
            let (mut canvas, paint) = canvas(mode);
            let serif = canvas
                .add_font_mem(include_bytes!("../../../../scenarios/clock/fonts/DejaVuSerif.ttf"))
                .unwrap();
            let mut layout = canvas.measure_text(32., 64., "AB", &paint).unwrap();
            let second = canvas
                .measure_text(32., 64., "CD", &paint.clone().with_font(&[serif]))
                .unwrap();
            layout.glyphs = vec![layout.glyphs[0], second.glyphs[0], layout.glyphs[1], second.glyphs[1]];
            for (i, glyph) in layout.glyphs.iter_mut().enumerate() {
                glyph.x = 32. + i as f32 * 24.;
            }
            canvas.prepare_glyph_cache();
            let mut state = std::mem::take(&mut canvas.cpu_glyphs);
            let commands = state.render(&mut canvas, &layout, 24., RenderMode::Fill).unwrap();
            let pixels = state.atlas.as_ref().unwrap().pages[0].pixels.clone();
            canvas.cpu_glyphs = state;
            canvas.draw_glyph_commands(commands, &paint, 1.);
            canvas.flush_to_surface(&());
            let first = glyph_cache_stats();
            let expected = if mode == GlyphCacheMode::Cpu { 4 } else { 2 };
            assert_eq!((first.face_parses, first.rasterized, first.uploads), (expected, 4, 1));
            let mut state = std::mem::take(&mut canvas.cpu_glyphs);
            assert!(state.render(&mut canvas, &layout, 24., RenderMode::Fill).is_some());
            canvas.cpu_glyphs = state;
            assert_eq!(glyph_cache_stats().face_parses, expected);
            assert_eq!(glyph_cache_stats().uploads, 1);
            // A later run with a new glyph parses again: faces are not retained
            // across runs or frames, while the existing masks remain reusable.
            draw(&mut canvas, &paint, "Z");
            assert_eq!(glyph_cache_stats().face_parses, expected + 1);
            drop(canvas);
            assert_no_residency();
            pixels
        }
        assert_eq!(mixed(GlyphCacheMode::Cpu), mixed(GlyphCacheMode::CpuRunFaces));
        assert_eq!(mixed(GlyphCacheMode::CpuRunFaces), mixed(GlyphCacheMode::CpuShapeFaces));
    }

    #[test]
    fn switching_cpu_policies_defers_reset_until_submitted_and_reuses_only_new_run_faces() {
        let (mut canvas, paint) = canvas(GlyphCacheMode::Cpu);
        draw(&mut canvas, &paint, "AB");
        let old = canvas.cpu_glyphs.atlas.as_ref().unwrap().pages[0].image;
        request_glyph_cache(GlyphCacheMode::CpuRunFaces);
        draw(&mut canvas, &paint, "CD");
        assert_eq!(glyph_cache_stats().face_parses, 4);
        assert!(canvas.images.get(old).is_some());
        canvas.flush_to_surface(&());
        draw(&mut canvas, &paint, "ABCD");
        assert_eq!(glyph_cache_stats().face_parses, 5);
        assert_eq!(glyph_cache_stats().resets, 2);
        assert!(canvas.images.get(old).is_none());
        canvas.flush_to_surface(&());
        draw(&mut canvas, &paint, "ABCD");
        assert_eq!(glyph_cache_stats().face_parses, 5);
    }

    #[test]
    fn run_face_mode_preserves_capacity_and_failure_fallback() {
        let (mut canvas, paint) = canvas(GlyphCacheMode::CpuRunFaces);
        canvas.prepare_glyph_cache();
        canvas.cpu_glyphs.atlas.as_mut().unwrap().entry_limit = 1;
        draw(&mut canvas, &paint, "AB");
        assert_eq!(glyph_cache_stats().entries, 1);
        assert_eq!(glyph_cache_stats().fallback_runs, 1);
        canvas.flush_to_surface(&());
        draw(&mut canvas, &paint, "AB");
        assert_eq!((glyph_cache_stats().entries, glyph_cache_stats().resets), (2, 2));
        canvas.flush_to_surface(&());
        canvas.renderer.fail_upload = true;
        draw(&mut canvas, &paint, "C");
        assert_eq!(glyph_cache_stats().errors, 1);
        canvas.flush_to_surface(&());
        let before = glyph_cache_stats().face_parses;
        draw(&mut canvas, &paint, "D");
        assert_eq!(glyph_cache_stats().face_parses, before);
        assert_eq!(glyph_cache_stats().fallback_runs, 3);
        canvas.flush_to_surface(&());
        request_glyph_cache(GlyphCacheMode::CpuRunFaces);
        draw(&mut canvas, &paint, "ABCD");
        assert_eq!(glyph_cache_stats().face_parses, before + 1);
        assert_eq!(glyph_cache_stats().entries, 4);
    }
}
