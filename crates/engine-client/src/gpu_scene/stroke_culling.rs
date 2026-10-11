//! Optional early rejection for the pinned Lyon fixed-width round polygon path.
//! This is deliberately wider than outline + stroke radius: inner joins matter.
use super::{
    mesh::Vertex,
    plan::{Draw, Shape},
    presentation::Clip,
};

pub(crate) use super::StrokeMode as Control;

#[derive(Clone, Copy, Debug)]
pub(super) struct Bounds {
    min: [f64; 2],
    max: [f64; 2],
}

impl Bounds {
    pub fn for_draw(draw: &Draw) -> Option<Self> {
        // Only the straight-edge, closed, fixed-width path used by mesh.rs.
        // Unknown shapes, extreme coordinates/widths and invalid styles fail open.
        let Shape::Polygon(points) = &draw.shape else {
            return None;
        };
        let stroke = draw.stroke?;
        if !(3..=256).contains(&points.len())
            || !(0.001..=256.).contains(&stroke.width)
            || ![
                stroke.color.r,
                stroke.color.g,
                stroke.color.b,
                stroke.color.a,
            ]
            .into_iter()
            .all(|c| (0.0..=1.0).contains(&c))
        {
            return None;
        }
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in points {
            for (i, v) in [p.x, p.y].into_iter().enumerate() {
                if !v.is_finite() || v.abs() > 16_384. {
                    return None;
                }
                min[i] = min[i].min(f64::from(v));
                max[i] = max[i].max(f64::from(v));
            }
        }
        // Lyon 1.0.20 stroke.rs::compute_join_side_positions_fixed_width:
        // round joins retain an inner miter only while its projection does not
        // exceed BOTH adjacent edges. A sharp join beyond both edges folds.
        // Thus use the whole path's L1 diameter, not merely half the width.
        // Merging only removes/replaces endpoints with existing path points.
        // Non-sharp inner joins and round arcs are bounded by the stroke width.
        // Four times (diameter + width) is intentionally loose; the restricted
        // numerical domain and two-pixel pad cover rounded normal/position math.
        // Revisit this argument if Lyon, stroke options, or input domain changes.
        let pad = 4. * (max[0] - min[0] + max[1] - min[1] + f64::from(stroke.width)) + 2.;
        Some(Self {
            min: [min[0] - pad, min[1] - pad],
            max: [max[0] + pad, max[1] + pad],
        })
    }

    pub fn outside(self, clip: Clip) -> bool {
        clip.width > 0
            && clip.height > 0
            && (self.max[0] < f64::from(clip.x) - 1.
                || self.max[1] < f64::from(clip.y) - 1.
                || self.min[0] > (i64::from(clip.x) + i64::from(clip.width)) as f64 + 1.
                || self.min[1] > (i64::from(clip.y) + i64::from(clip.height)) as f64 + 1.)
    }

    #[cfg(test)]
    fn contains(self, v: &Vertex) -> bool {
        (0..2).all(|i| {
            v[i].is_finite() && f64::from(v[i]) >= self.min[i] && f64::from(v[i]) <= self.max[i]
        })
    }
}

/// Independent output check, also used by the benchmark's checked control.
pub(crate) fn vertices_outside(vertices: &[Vertex], clip: Clip) -> bool {
    if vertices.is_empty() {
        return true;
    }
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for v in vertices {
        if !v.iter().all(|x| x.is_finite()) {
            return false;
        }
        for i in 0..2 {
            min[i] = min[i].min(f64::from(v[i]));
            max[i] = max[i].max(f64::from(v[i]));
        }
    }
    Bounds { min, max }.outside(clip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_scene::mesh::MeshBuilder;
    use engine_common::{RenderColor, RenderPoint, Stroke};

    fn draw(points: &[[f32; 2]], width: f32) -> Draw {
        Draw {
            shape: Shape::Polygon(
                points
                    .iter()
                    .map(|p| RenderPoint::new(p[0], p[1]))
                    .collect(),
            ),
            fill: None,
            stroke: Some(Stroke::new(RenderColor::WHITE, width)),
        }
    }

    #[test]
    fn restricted_domain_fails_open_and_clip_edges_keep_strokes() {
        let mut d = draw(&[[0., 0.], [1., 0.], [0., 1.]], 1.);
        let clip = Clip {
            x: 20,
            y: 20,
            width: 100,
            height: 100,
        };
        assert!(Bounds::for_draw(&d).unwrap().outside(clip));
        for width in [0., -1., f32::NAN, f32::INFINITY, 257., f32::MIN_POSITIVE] {
            d.stroke.as_mut().unwrap().width = width;
            assert!(Bounds::for_draw(&d).is_none());
        }
        d.stroke.as_mut().unwrap().width = 1.;
        d.stroke.as_mut().unwrap().color.r = f32::NAN;
        assert!(Bounds::for_draw(&d).is_none());
        for v in [16_385., f32::INFINITY, f32::NAN] {
            assert!(Bounds::for_draw(&draw(&[[v, 0.], [1., 0.], [0., 1.]], 1.)).is_none());
        }
        for count in [0, 1, 2, 257] {
            assert!(Bounds::for_draw(&draw(&vec![[0., 0.]; count], 1.)).is_none());
        }
        let d = draw(&[[19., 40.], [19., 60.], [18., 50.]], 12.);
        let bounds = Bounds::for_draw(&d).unwrap();
        assert!(!bounds.outside(clip));
        assert!(!bounds.outside(Clip { width: 0, ..clip }));
        assert!(!bounds.outside(Clip { height: -1, ..clip }));
        let mut mesh = MeshBuilder::default();
        mesh.append_draw(&d).unwrap();
        assert!(!vertices_outside(&mesh.buffers.vertices, clip));
    }

    #[test]
    fn envelope_contains_actual_lyon_output_for_merges_folds_and_transforms() {
        let shapes = [
            vec![[0., 0.], [4., 0.], [1., 3.]],
            vec![[0., 0.], [4., 0.], [0.001, 0.002]],
            vec![[0., 0.], [4., 0.], [0., 0.], [-4., 0.]],
            vec![[0., 0.]; 4],
            vec![[0., 0.], [0.0001, 0.], [0.0002, 0.0001], [0., 0.0002]],
            vec![[0., 0.], [4., 4.], [0., 4.], [4., 0.]],
            vec![[0., 0.], [4., 0.], [1., 0.1], [3., 3.], [0., 1.]],
            vec![[0., 0.], [4., 0.], [0.00001, 0.00001], [0., 0.]],
        ];
        let mut mesh = MeshBuilder::default();
        let mut cases = 0;
        for shape in shapes {
            for width in [0.001, 0.01, 0.2, 1., 12., 256.] {
                for scale in [0.0001, 0.01, 0.1, 1., 32., 512.] {
                    for angle in [0., 0.03, 0.71, 1.57, 3.12] {
                        for shift in [0., 100., -16_000., 16_000.] {
                            let (s, c) = f32::sin_cos(angle);
                            let points: Vec<_> = shape
                                .iter()
                                .map(|[x, y]| {
                                    [
                                        shift + scale * (c * x - s * y),
                                        shift + scale * (s * x + c * y),
                                    ]
                                })
                                .collect();
                            let d = draw(&points, width);
                            let Some(bounds) = Bounds::for_draw(&d) else {
                                continue;
                            };
                            mesh.clear();
                            mesh.append_draw(&d).unwrap();
                            assert!(
                                mesh.buffers.vertices.iter().all(|v| bounds.contains(v)),
                                "bounds={bounds:?}, shape={points:?}, width={width}"
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
        assert!(cases > 5_000);
    }

    #[test]
    fn seeded_irregular_outlines_remain_within_envelope() {
        let mut seed = 0x5472_2319_u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / 16_777_216.
        };
        let mut mesh = MeshBuilder::default();
        for i in 0..8_000 {
            let scale = [0.0001, 0.01, 0.1, 1., 100.][i % 5];
            let shift = if i % 7 == 0 {
                16_000.
            } else {
                -400. + 800. * next()
            };
            let points: Vec<_> = (0..3 + i % 29)
                .map(|j| {
                    let x = shift + (next() - 0.5) * scale;
                    let y = shift
                        + if j % 3 == 0 {
                            0.
                        } else {
                            (next() - 0.5) * scale
                        };
                    [x, y]
                })
                .collect();
            let d = draw(&points, [0.001, 0.1, 1., 12., 256.][(i / 5) % 5]);
            let bounds = Bounds::for_draw(&d).unwrap();
            mesh.clear();
            mesh.append_draw(&d).unwrap();
            assert!(
                mesh.buffers.vertices.iter().all(|v| bounds.contains(v)),
                "case={i}, bounds={bounds:?}, draw={d:?}"
            );
        }
    }
}
