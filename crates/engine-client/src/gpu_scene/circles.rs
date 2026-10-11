//! Bounded shared circle templates. Instances carry placement/style, never mesh
//! identity. Dynamic polygons stay on the ordinary streamed triangle path.
use super::plan::Draw;
use engine_common::RenderColor;

pub(super) type Instance = [f32; 12]; // center.xy, radius, width, fill.rgba, stroke.rgba
pub(super) type Vertex = [f32; 3]; // unit direction.xy, role (center/fill/inner/outer)
pub(super) const LEVELS: [usize; 8] = [8, 16, 32, 64, 128, 256, 512, 1024];

// Conservative sagitta bound r * pi² / (2*n²), at most 1/8 logical pixel.
// The whole consecutive run uses its highest required level, keeping ordering
// and batching without sorting overlapping transparent circles by size.
pub(super) fn level(radius: f32) -> Option<usize> {
    LEVELS.iter().position(|&n| {
        radius.is_finite()
            && radius >= 0.0
            && radius * std::f32::consts::PI.powi(2) <= 0.25 * (n * n) as f32
    })
}

pub(super) fn instance(draw: &Draw, x: f32, y: f32, radius: f32) -> Instance {
    let premul =
        |c: Option<RenderColor>| c.map_or([0.0; 4], |c| [c.r * c.a, c.g * c.a, c.b * c.a, c.a]);
    let f = premul(draw.fill);
    let s = premul(draw.stroke.map(|s| s.color));
    [
        x,
        y,
        radius,
        draw.stroke.map_or(0.0, |s| s.width),
        f[0],
        f[1],
        f[2],
        f[3],
        s[0],
        s[1],
        s[2],
        s[3],
    ]
}

pub(super) fn template(segments: usize) -> (Vec<Vertex>, Vec<u32>) {
    let mut vertices = vec![[0.0, 0.0, 0.0]];
    for role in [1.0, 2.0, 3.0] {
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            vertices.push([angle.cos(), angle.sin(), role]);
        }
    }
    let mut indices = Vec::with_capacity(segments * 9);
    // Fill before stroke, including their intentional overlap when transparent.
    for i in 0..segments as u32 {
        indices.extend([0, 1 + i, 1 + (i + 1) % segments as u32]);
    }
    for i in 0..segments as u32 {
        let next = (i + 1) % segments as u32;
        let inner = 1 + segments as u32;
        let outer = inner + segments as u32;
        indices.extend([
            inner + i,
            outer + i,
            outer + next,
            inner + i,
            outer + next,
            inner + next,
        ]);
    }
    (vertices, indices)
}

// Fixed-layout control: reuse the streamed triangle's xy/rgba format. Plan
// colors have alpha in [0, 1], so negative alpha can tag the four circle roles
// without changing triangle uploads or needing a per-batch mode uniform.
pub(super) fn layout_vertex(v: Vertex) -> super::mesh::Vertex {
    [v[0], v[1], 0.0, 0.0, 0.0, -(v[2] + 1.0)]
}

// CPU counterpart of the instance vertex shader, for the expansion ablation.
pub(super) fn expand_vertex(v: Vertex, instance: &Instance) -> super::mesh::Vertex {
    let [x, y, radius, width, fr, fg, fb, fa, sr, sg, sb, sa] = *instance;
    let stroke = v[2] > 1.5;
    let r = if stroke {
        (radius
            + if v[2] > 2.5 {
                width * 0.5
            } else {
                -width * 0.5
            })
        .max(0.0)
    } else if fa == 0.0 {
        0.0
    } else {
        radius
    };
    let [r_color, g, b, a] = if stroke {
        [sr, sg, sb, sa]
    } else {
        [fr, fg, fb, fa]
    };
    [x + v[0] * r, y + v[1] * r, r_color, g, b, a]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_bound_screen_space_error_and_reject_unbounded_geometry() {
        for radius in [0.01, 0.5, 1.0, 10.0, 100.0, 1000.0, 16000.0] {
            let n = LEVELS[level(radius).unwrap()];
            assert!(radius * (1.0 - (std::f32::consts::PI / n as f32).cos()) <= 0.126);
        }
        for invalid in [f32::NAN, f32::INFINITY, -1.0, 1e9] {
            assert!(level(invalid).is_none());
        }
    }

    #[test]
    fn templates_are_bounded_and_keep_fill_before_stroke() {
        for n in LEVELS {
            let (vertices, indices) = template(n);
            assert_eq!(vertices.len(), 1 + 3 * n);
            assert_eq!(indices.len(), 9 * n);
            assert!(indices.iter().all(|&i| i < vertices.len() as u32));
            assert!(
                indices[..3 * n]
                    .iter()
                    .all(|&i| vertices[i as usize][2] <= 1.0)
            );
            assert!(
                indices[3 * n..]
                    .iter()
                    .all(|&i| vertices[i as usize][2] >= 2.0)
            );
            assert!(vertices.iter().flatten().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn fixed_layout_preserves_positions_and_unambiguously_encodes_roles() {
        assert_eq!(std::mem::size_of::<super::super::mesh::Vertex>(), 24);
        let mut bytes = 0;
        for n in LEVELS {
            let (vertices, indices) = template(n);
            bytes += vertices.len() * 24 + indices.len() * 4;
            for v in vertices {
                let encoded = layout_vertex(v);
                assert_eq!(&encoded[..2], &v[..2]);
                assert!(encoded[5] < 0.0);
                assert_eq!(-encoded[5] - 1.0, v[2]);
                assert_eq!(&encoded[2..5], &[0.0; 3]);
            }
        }
        assert_eq!(bytes, 220512);
    }

    #[test]
    fn expansion_matches_shader_roles_including_missing_fill_and_thick_stroke() {
        let mut instance = [10., 20., 2., 6., 0.5, 0.25, 0., 0.5, 0., 0., 0.25, 0.25];
        assert_eq!(
            expand_vertex([0., 0., 0.], &instance),
            [10., 20., 0.5, 0.25, 0., 0.5]
        );
        assert_eq!(
            expand_vertex([1., 0., 1.], &instance),
            [12., 20., 0.5, 0.25, 0., 0.5]
        );
        assert_eq!(
            expand_vertex([1., 0., 2.], &instance),
            [10., 20., 0., 0., 0.25, 0.25]
        );
        assert_eq!(
            expand_vertex([0., 1., 3.], &instance),
            [10., 25., 0., 0., 0.25, 0.25]
        );
        instance[4..8].fill(0.0);
        assert_eq!(
            expand_vertex([1., 0., 1.], &instance),
            [10., 20., 0., 0., 0., 0.]
        );
    }
}
