//! Deliberately bounded, fill-only atlas for the offscreen experiment. Same
//! em scale, padding, x quantization, bearings and nearest sampling as FemtoVG
//! 0.17.0, but analytical CPU coverage instead of eight vector samples.
use super::{Error, GlCanvas};
use ab_glyph_rasterizer::{Point, Rasterizer, point};
use femtovg::{Atlas, DrawCommand, GlyphDrawCommands, ImageFlags, ImageId, ImageSource, Quad};
use std::collections::HashMap;
use ttf_parser::{Face, GlyphId, OutlineBuilder};

const SIDE: usize = 512;
const MAX_PAGES: usize = 4;
type Pixel = femtovg::rgb::RGBA8;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct Key {
    font: usize,
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
    pixels: Vec<Pixel>,
    image: ImageId,
    dirty: Option<(usize, usize)>,
}
#[derive(Default)]
pub(super) struct CpuAtlas {
    pages: Vec<Page>,
    glyphs: HashMap<Key, Entry>,
}

// This fixture uses positive local coordinates. Refuse negative positions
// instead of pretending to implement FemtoVG's asymmetric negative quantizer.
fn subpixel(x: f32) -> Result<u8, Error> {
    if !x.is_finite() || x < 0. {
        return Err("CPU mask probe requires nonnegative finite glyph x".into());
    }
    Ok((x.fract() / 0.1 + 0.5).trunc() as u8)
}
fn dimensions(width: f32, height: f32) -> Result<(usize, usize), Error> {
    if !width.is_finite() || !height.is_finite() || width < 0. || height < 0. {
        return Err("invalid glyph dimensions".into());
    }
    if width > (SIDE - 4) as f32 || height > (SIDE - 4) as f32 {
        return Err("glyph exceeds the bounded probe atlas".into());
    }
    let result = (width.ceil() as usize + 4, height.ceil() as usize + 4);
    if result.0 > SIDE || result.1 > SIDE {
        return Err("glyph exceeds the bounded probe atlas".into());
    }
    Ok(result)
}

impl CpuAtlas {
    pub(super) fn len(&self) -> usize {
        self.glyphs.len()
    }
    pub(super) fn pages(&self) -> usize {
        self.pages.len()
    }
    pub(super) fn commands(
        &mut self,
        canvas: &mut GlCanvas,
        face: &Face<'_>,
        font: usize,
        size: f32,
        layout: &femtovg::TextMetrics,
    ) -> Result<GlyphDrawCommands, Error> {
        if !size.is_finite() || !(1. ..=92.).contains(&size) {
            return Err("CPU mask probe supports fill sizes 1..92 only".into());
        }
        let mut grouped = Vec::<DrawCommand>::new();
        for glyph in layout.glyphs.iter().filter(|g| !g.c.is_control()) {
            if glyph.bitmap_glyph {
                return Err("bitmap/color glyphs are outside this experiment".into());
            }
            let key = Key {
                font,
                glyph: glyph.codepoint.try_into()?,
                size: size.to_bits(),
                subpixel: subpixel(glyph.x)?,
            };
            let entry = if let Some(entry) = self.glyphs.get(&key) {
                *entry
            } else {
                let (w, h) = dimensions(glyph.width, glyph.height)?;
                let (page, x, y) = self.allocate(canvas, w, h)?;
                let mut outline = Outline::new(
                    w,
                    h,
                    size / f32::from(face.units_per_em()),
                    2. - glyph.bearing_x + f32::from(key.subpixel) * 0.1,
                    2. + glyph.bearing_y.round(),
                );
                // Spaces legitimately have no outline; a visible glyph without
                // one must fail instead of silently disappearing.
                if face
                    .outline_glyph(GlyphId(key.glyph), &mut outline)
                    .is_none()
                    && (glyph.width > 0. || glyph.height > 0.)
                {
                    return Err(format!("missing outline for glyph {}", key.glyph).into());
                }
                let page_ref = &mut self.pages[page];
                outline.raster.for_each_pixel(|i, coverage| {
                    let value = (coverage.clamp(0., 1.) * 255.).round() as u8;
                    page_ref.pixels[(y + i / w) * SIDE + x + i % w] =
                        Pixel::new(value, value, value, 255);
                });
                page_ref.dirty = Some(match page_ref.dirty {
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
                self.glyphs.insert(key, entry);
                entry
            };
            let image_id = self.pages[entry.page].image;
            let index = grouped.iter().position(|c| c.image_id == image_id);
            let index = index.unwrap_or_else(|| {
                grouped.push(DrawCommand {
                    image_id,
                    quads: Vec::new(),
                });
                grouped.len() - 1
            });
            let x = glyph.x.trunc() - 1.;
            let y = (glyph.y + glyph.bearing_y).round() - entry.bearing_y - 1.;
            grouped[index].quads.push(Quad {
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
        Ok(GlyphDrawCommands {
            alpha_glyphs: grouped,
            color_glyphs: Vec::new(),
        })
    }

    fn allocate(
        &mut self,
        canvas: &mut GlCanvas,
        w: usize,
        h: usize,
    ) -> Result<(usize, usize, usize), Error> {
        for (index, page) in self.pages.iter_mut().enumerate() {
            if let Some((x, y)) = page.packing.add_rect(w, h) {
                return Ok((index, x, y));
            }
        }
        if self.pages.len() == MAX_PAGES {
            return Err(
                "bounded CPU mask atlas exhausted (4 pages); no eviction in this probe".into(),
            );
        }
        let image = canvas.create_image_empty(
            SIDE,
            SIDE,
            femtovg::PixelFormat::Rgba8,
            ImageFlags::NEAREST,
        )?;
        let mut packing = Atlas::new(SIDE, SIDE);
        let (x, y) = packing.add_rect(w, h).ok_or("glyph did not fit new page")?;
        self.pages.push(Page {
            packing,
            pixels: vec![Pixel::new(0, 0, 0, 255); SIDE * SIDE],
            image,
            dirty: None,
        });
        Ok((self.pages.len() - 1, x, y))
    }

    // One contiguous row-band upload per dirty page, after ALL rows have been
    // generated. No per-glyph GL calls and no CPU repacking allocation/copy.
    pub(super) fn upload(&mut self, canvas: &mut GlCanvas) -> Result<usize, Error> {
        let mut uploads = 0;
        for page in &mut self.pages {
            if let Some((top, bottom)) = page.dirty.take() {
                let data = femtovg::imgref::ImgRef::new(
                    &page.pixels[top * SIDE..bottom * SIDE],
                    SIDE,
                    bottom - top,
                );
                canvas.update_image(page.image, ImageSource::Rgba(data), 0, top)?;
                uploads += 1;
            }
        }
        Ok(uploads)
    }
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
        point(
            self.offset.x + x * self.scale,
            self.offset.y - y * self.scale,
        )
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
    #[test]
    fn quantization_matches_positive_femtovg_positions() {
        assert_eq!(subpixel(12.24).unwrap(), 2);
        assert_eq!(subpixel(12.26).unwrap(), 3);
        assert_eq!(subpixel(12.99).unwrap(), 10);
        assert!(subpixel(-0.1).is_err());
        assert!(subpixel(f32::NAN).is_err());
    }
    #[test]
    fn glyph_bounds_include_padding_and_reject_oversize() {
        assert_eq!(dimensions(12.3, 18.8).unwrap(), (17, 23));
        assert_eq!(dimensions(0., 0.).unwrap(), (4, 4));
        assert!(dimensions(509., 1.).is_err());
        assert!(dimensions(f32::INFINITY, 1.).is_err());
        assert!(dimensions(-1., 1.).is_err());
    }
    #[test]
    fn outline_applies_y_flip_and_preserves_fractional_coverage() {
        let mut outline = Outline::new(8, 8, 1., 2.25, 5.);
        outline.move_to(0., 0.);
        outline.line_to(2., 0.);
        outline.line_to(2., 2.);
        outline.line_to(0., 2.);
        outline.close();
        let mut pixels = [0.; 64];
        outline.raster.for_each_pixel(|i, a| pixels[i] = a);
        assert!((pixels.iter().sum::<f32>() - 4.).abs() < 1e-6);
        assert!((pixels[3 * 8 + 2] - 0.75).abs() < 1e-6);
        assert!((pixels[3 * 8 + 4] - 0.25).abs() < 1e-6);
        assert_eq!(pixels[2 * 8 + 2], 0.);
    }
}
