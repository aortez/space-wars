//! Vertical shadow envelope. Every output trapezoid lies above every silhouette
//! edge in its x interval; no center-ray/AABB approximation can fill a hidden gap.
use super::*;
use engine_core::lighting::ApertureSource;

const MAX_EDGES: usize = 2048;
const MAX_BREAKS: usize = 4096;
const MAX_WORK: usize = 1_000_000;

#[derive(Clone, Copy)]
struct Edge {
    left: f64,
    right: f64,
    slope: f64,
    intercept: f64,
}
impl Edge {
    fn y(self, x: f64) -> f64 {
        self.slope * x + self.intercept
    }
}

/// Clip at the source plane before projecting shadows: geometry wholly above
/// the ceiling cannot shadow a receiver whose light ray ends at that ceiling.
fn below_ceiling(points: &[Vec2], ceiling: f32) -> Vec<Vec2> {
    let mut out = Vec::new();
    let count = if points.len() == 2 { 1 } else { points.len() };
    for i in 0..count {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        if a.y <= ceiling {
            out.push(a);
        }
        if (a.y <= ceiling) != (b.y <= ceiling) {
            let t = (ceiling as f64 - a.y as f64) / (b.y as f64 - a.y as f64);
            out.push(Vec2::new(
                (a.x as f64 + t * (b.x as f64 - a.x as f64)) as f32,
                ceiling,
            ));
        }
        if points.len() == 2 && b.y <= ceiling {
            out.push(b);
        }
    }
    out
}

pub(super) fn polygons(
    source: ApertureSource,
    floor: f32,
    silhouettes: &[Vec<Vec2>],
) -> Option<Vec<Vec<RenderPoint>>> {
    if silhouettes.iter().map(Vec::len).sum::<usize>() > 32_768 {
        return None;
    }
    let left = source.left as f64;
    let right = source.right as f64;
    let floor = floor as f64;
    let ceiling = source.ceiling_y as f64;
    let mut edges = Vec::new();
    for polygon in silhouettes {
        if polygon.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return None;
        }
        let points = below_ceiling(polygon, source.ceiling_y);
        let count = if points.len() == 2 { 1 } else { points.len() };
        for i in 0..count {
            let a = points[i];
            let b = points[(i + 1) % points.len()];
            let x0 = (a.x.min(b.x) as f64).max(left);
            let x1 = (a.x.max(b.x) as f64).min(right);
            if x0 >= x1 || (a.y.max(b.y) as f64) < floor {
                continue;
            }
            let slope = (b.y as f64 - a.y as f64) / (b.x as f64 - a.x as f64);
            edges.push(Edge {
                left: x0,
                right: x1,
                slope,
                intercept: a.y as f64 - slope * a.x as f64,
            });
            if edges.len() > MAX_EDGES {
                return None;
            }
        }
    }
    let mut cuts = vec![left, right];
    for e in &edges {
        cuts.extend([e.left, e.right]);
        // Clamping at the floor/ceiling can also change the upper envelope.
        if e.slope != 0.0 {
            for y in [floor, ceiling] {
                let x = (y - e.intercept) / e.slope;
                if e.left < x && x < e.right {
                    cuts.push(x);
                }
            }
        }
    }
    let mut work = 0;
    for (i, a) in edges.iter().enumerate() {
        for b in &edges[..i] {
            work += 1;
            if work > MAX_WORK {
                return None;
            }
            if a.slope == b.slope {
                continue;
            }
            let x = (b.intercept - a.intercept) / (a.slope - b.slope);
            if a.left.max(b.left) < x && x < a.right.min(b.right) {
                cuts.push(x);
            }
            if cuts.len() > MAX_BREAKS {
                return None;
            }
        }
    }
    if cuts.len() > MAX_BREAKS {
        return None;
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup();
    let mut output = Vec::new();
    for interval in cuts.windows(2) {
        let (a, b) = (interval[0], interval[1]);
        let midpoint = (a + b) * 0.5;
        let mut top = None;
        let mut height = floor;
        for &e in &edges {
            work += 1;
            if work > MAX_WORK {
                return None;
            }
            if e.left <= midpoint && midpoint <= e.right && e.y(midpoint) > height {
                height = e.y(midpoint);
                top = Some(e);
            }
        }
        if height >= ceiling {
            continue;
        }
        let y = |x| top.map_or(floor, |e: Edge| e.y(x)).clamp(floor, ceiling) as f32;
        let (a32, b32) = (a as f32, b as f32);
        if a32 >= b32 {
            continue;
        }
        output.push(vec![
            RenderPoint::new(a32, y(a)),
            RenderPoint::new(b32, y(b)),
            RenderPoint::new(b32, ceiling as f32),
            RenderPoint::new(a32, ceiling as f32),
        ]);
    }
    Some(output)
}

/// One outline removes all internal tessellation edges before either renderer
/// applies alpha. Vertical steps preserve discontinuities in the shadow envelope.
pub(super) fn outline(pieces: &[Vec<RenderPoint>]) -> Vec<RenderPoint> {
    let Some(first) = pieces.first() else {
        return Vec::new();
    };
    let mut points = vec![first[3]];
    for (index, piece) in pieces.iter().enumerate() {
        points.extend_from_slice(&piece[..2]);
        // A missing interval is fully blocked up to the source plane.
        // Connect across it along the ceiling, never across the hidden region.
        if let Some(next) = pieces.get(index + 1)
            && piece[1].x < next[0].x
        {
            points.extend([piece[2], next[3]]);
        }
    }
    points.push(pieces.last().unwrap()[2]);
    points
}
