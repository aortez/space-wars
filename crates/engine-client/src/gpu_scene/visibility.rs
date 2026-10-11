//! Conservative bounds of cached triangles, not guesses from source outlines.
//! Culling is optional; uncertain/non-finite bounds always keep the geometry.
use super::{mesh::Vertex, presentation::Clip, retained::Placement};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bounds {
    min: [f32; 4],
    max: [f32; 4],
}

impl Bounds {
    pub fn new(points: impl Iterator<Item = [f32; 4]>) -> Self {
        let mut bounds = Self {
            min: [f32::INFINITY; 4],
            max: [f32::NEG_INFINITY; 4],
        };
        for p in points {
            if !p.iter().all(|v| v.is_finite()) {
                return Self {
                    min: [f32::NAN; 4],
                    max: [f32::NAN; 4],
                };
            }
            for (i, v) in p.into_iter().enumerate() {
                bounds.min[i] = bounds.min[i].min(v);
                bounds.max[i] = bounds.max[i].max(v);
            }
        }
        bounds
    }

    pub fn outside(&self, scale: f32, (s, c): (f32, f32), p: &Placement, clip: Clip) -> bool {
        // Every operation has the same order as CPU expansion. Positive scale
        // and monotonic IEEE operations enclose the actual rounded vertices;
        // signed multiplication chooses the appropriate interval endpoint.
        let x = [
            self.min[0] * scale + self.min[2],
            self.max[0] * scale + self.max[2],
        ];
        let y = [
            self.min[1] * scale + self.min[3],
            self.max[1] * scale + self.max[3],
        ];
        let interval = |a: f32, b: f32, t: f32| {
            let ax = [a * x[0], a * x[1]];
            let by = [b * y[0], b * y[1]];
            [
                ax[0].min(ax[1]) + by[0].min(by[1]) + t,
                ax[0].max(ax[1]) + by[0].max(by[1]) + t,
            ]
        };
        let bx = interval(c, s, p.translation.x);
        let by = interval(-s, c, p.translation.y);
        // One extra pixel protects clip-edge MSAA/rasterization rounding.
        // Never let min/max's NaN handling accidentally turn overflow into a
        // rejection. No assumption about zoom, shape, winding or join width.
        scale > 0.
            && self
                .min
                .iter()
                .chain(&self.max)
                .chain(&x)
                .chain(&y)
                .chain(&bx)
                .chain(&by)
                .chain([&s, &c])
                .all(|v| v.is_finite())
            && (bx[1] < clip.x as f32 - 1.
                || by[1] < clip.y as f32 - 1.
                || bx[0] > (i64::from(clip.x) + i64::from(clip.width)) as f32 + 1.
                || by[0] > (i64::from(clip.y) + i64::from(clip.height)) as f32 + 1.)
    }
}

#[derive(Clone, Debug)]
pub(super) struct Span {
    pub vertices: Range<usize>,
    pub indices: Range<usize>,
}

struct Chunk {
    span: Span,
    bounds: Bounds,
}

#[derive(Default)]
pub(super) struct Chunks(Vec<Chunk>);
impl Chunks {
    pub fn new(vertices: &[Vertex], indices: &[u32]) -> Self {
        // Index boundaries are triangle boundaries. Bounds include ALL vertices
        // in the copied interval (including unreferenced ones), keeping finite
        // validation and expansion behavior conservative.
        const INDICES: usize = 384;
        Self(
            indices
                .chunks(INDICES)
                .enumerate()
                .map(|(i, ids)| {
                    let first = *ids.iter().min().unwrap() as usize;
                    let end = *ids.iter().max().unwrap() as usize + 1;
                    Chunk {
                        bounds: Bounds::new(
                            vertices[first..end].iter().map(|v| [v[0], v[1], 0., 0.]),
                        ),
                        span: Span {
                            vertices: first..end,
                            indices: i * INDICES..i * INDICES + ids.len(),
                        },
                    }
                })
                .collect(),
        )
    }

    pub fn bytes(&self) -> usize {
        self.0.capacity() * std::mem::size_of::<Chunk>()
    }

    pub fn select(&self, p: &Placement, sc: (f32, f32), clip: Clip, out: &mut Vec<Span>) -> u64 {
        out.clear();
        let mut culled = 0;
        for chunk in &self.0 {
            if chunk.bounds.outside(1., sc, p, clip) {
                culled += 1;
                continue;
            }
            if let Some(last) = out
                .last_mut()
                .filter(|last| last.indices.end == chunk.span.indices.start)
            {
                last.indices.end = chunk.span.indices.end;
                last.vertices.start = last.vertices.start.min(chunk.span.vertices.start);
                last.vertices.end = last.vertices.end.max(chunk.span.vertices.end);
            } else {
                out.push(chunk.span.clone());
            }
        }
        culled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::{PolygonGeometry, RenderColor, RenderPoint, RenderPolygon};
    use std::sync::Arc;
    fn pose() -> Placement {
        Placement {
            first: 0,
            definition: Arc::new(
                PolygonGeometry::new(vec![RenderPolygon::filled(
                    vec![
                        RenderPoint::ZERO,
                        RenderPoint::new(1., 0.),
                        RenderPoint::new(0., 1.),
                    ],
                    RenderColor::WHITE,
                )])
                .unwrap(),
            ),
            scale: 1.,
            angle: 0.,
            translation: RenderPoint::ZERO,
        }
    }
    #[test]
    fn clip_margin_strokes_rotations_and_invalid_bounds_are_conservative() {
        let clip = Clip {
            x: 10,
            y: 20,
            width: 100,
            height: 80,
        };
        let mut p = pose();
        let bounds = Bounds::new([[0., 0., -5., -5.], [2., 2., 5., 5.]].into_iter());
        assert!(bounds.outside(1., (0., 1.), &p, clip));
        p.translation = RenderPoint::new(50., 50.);
        assert!(!bounds.outside(1., (0., 1.), &p, clip));
        p.translation = RenderPoint::new(115., 50.);
        assert!(!bounds.outside(1., (0., 1.), &p, clip));
        p.translation.x = 117.;
        assert!(bounds.outside(1., (0., 1.), &p, clip));
        assert!(!bounds.outside(f32::INFINITY, (0., 1.), &p, clip));
        assert!(!Bounds::new([[f32::NAN; 4]].into_iter()).outside(1., (0., 1.), &p, clip));
        assert!(!Bounds::new(std::iter::empty()).outside(1., (0., 1.), &p, clip));
        // Compare all sampled actual vertices against rejected intervals.
        for scale in [0.001, 0.2, 1., 23., 10000.] {
            for i in 0..100 {
                let sc = (i as f32 * 0.173).sin_cos();
                p.translation.x = i as f32 * 3. - 100.;
                if bounds.outside(scale, sc, &p, clip) {
                    for j in 0..100 {
                        let f = j as f32 / 99.;
                        let x = 2. * f * scale + 5. - 10. * f;
                        let y = 2. * (1. - f) * scale - 5. + 10. * f;
                        let v = [
                            sc.1 * x + sc.0 * y + p.translation.x,
                            -sc.0 * x + sc.1 * y + p.translation.y,
                        ];
                        assert!(v[0] < 10. || v[0] > 110. || v[1] < 20. || v[1] > 100.);
                    }
                }
            }
        }
    }
    #[test]
    fn chunk_selection_preserves_index_order_and_can_reveal_removed_geometry() {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for x in [20., 1000., 40.] {
            for i in 0..384 {
                indices.push(vertices.len() as u32);
                vertices.push([x, 40. + (i % 3) as f32, 1., 1., 1., 1.]);
            }
        }
        let chunks = Chunks::new(&vertices, &indices);
        let mut spans = Vec::new();
        let mut p = pose();
        let clip = Clip {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        assert_eq!(chunks.select(&p, (0., 1.), clip, &mut spans), 1);
        assert_eq!(
            spans.iter().map(|s| s.indices.clone()).collect::<Vec<_>>(),
            [0..384, 768..1152]
        );
        p.translation.x = -950.;
        assert_eq!(chunks.select(&p, (0., 1.), clip, &mut spans), 2);
        assert_eq!(spans[0].indices, 384..768);
        assert!(chunks.bytes() > 0);
    }
}
