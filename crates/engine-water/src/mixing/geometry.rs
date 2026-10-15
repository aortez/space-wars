//! Retained sheets collide on their material pieces. A velocity-normal box
//! rotates a deep vertical face into the gap and cancels its motion too early.
use super::*;

pub(super) fn bounds(ribbon: &SpillRibbon, travel: Vec2) -> (Vec2, Vec2) {
    let mut min = Vec2::new(f32::INFINITY, f32::INFINITY);
    let mut max = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &point in ribbon.quads().iter().flatten() {
        for p in [point, point + travel] {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
    }
    (min, max)
}

pub(super) fn contact(a: &SpillRibbon, b: &SpillRibbon, relative: Vec2, dt: f64) -> Option<f64> {
    a.quads()
        .iter()
        .flat_map(|qa| {
            b.quads()
                .iter()
                .filter_map(move |qb| pieces_contact(qa, qb, relative, dt))
        })
        .min_by(f64::total_cmp)
}

fn pieces_contact(a: &[Vec2; 4], b: &[Vec2; 4], relative: Vec2, dt: f64) -> Option<f64> {
    let mut enter = 0.0_f64;
    let mut exit = dt;
    // Freeze each convex piece for this sweep. Both share the same gravity,
    // which cancels from relative translation, including newborn rewind.
    for quad in [a, b] {
        for i in 0..4 {
            let axis = normal(quad[(i + 1) % 4] - quad[i]);
            if axis.length_squared() <= 1e-12 {
                continue; // Triangles repeat a corner.
            }
            let project = |q: &[Vec2; 4]| {
                q.iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                        let d = p.dot(axis) as f64;
                        (lo.min(d), hi.max(d))
                    })
            };
            let (amin, amax) = project(a);
            let (bmin, bmax) = project(b);
            let speed = relative.dot(axis) as f64;
            if speed.abs() < 1e-9 {
                if amax < bmin || bmax < amin {
                    return None;
                }
            } else {
                let t0 = (amin - bmax) / speed;
                let t1 = (amax - bmin) / speed;
                enter = enter.max(t0.min(t1));
                exit = exit.min(t0.max(t1));
                if enter > exit {
                    return None;
                }
            }
        }
    }
    Some(enter)
}
