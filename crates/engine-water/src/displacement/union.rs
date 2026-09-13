//! Build the exposed boundary of a bounded union of convex hulls. Split each
//! edge at its covered intervals, with one deterministic owner for coincident
//! edges. Oppositely oriented shared edges disappear from both polygons.
//!
//! Green's theorem gives clipped area as integral (h-y) dx over the boundary
//! below h. A horizontal closing edge at h contributes zero, as do vertical
//! column boundaries. Thus the same cached segments give the union's capacity
//! and each column's occupied area, without inclusion/exclusion explosions or
//! recomputing a polygon union during every reference-level iteration.
use super::{DisplacementBody, MAX_DISPLACERS};
use crate::{
    PoolSpec,
    immersion::{WaterHull, clip},
};

type Point = [f64; 2];
const VERTICES: usize = crate::immersion::CLIPPED_VERTICES;
const MAX_SEGMENTS: usize = MAX_DISPLACERS * MAX_DISPLACERS * VERTICES;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Edge {
    a: Point,
    b: Point,
}

impl Edge {
    fn at(self, t: f64) -> Point {
        [
            self.a[0] + t * (self.b[0] - self.a[0]),
            self.a[1] + t * (self.b[1] - self.a[1]),
        ]
    }

    fn below(self, height: f64) -> Option<Self> {
        if self.a[1] >= height && self.b[1] >= height {
            return None;
        }
        let mut edge = self;
        if self.a[1] > height {
            edge.a = self.at((height - self.a[1]) / (self.b[1] - self.a[1]));
        } else if self.b[1] > height {
            edge.b = self.at((height - self.a[1]) / (self.b[1] - self.a[1]));
        }
        Some(edge)
    }

    fn integral(self, height: f64) -> f64 {
        (self.b[0] - self.a[0]) * (height - (self.a[1] + self.b[1]) * 0.5)
    }

    fn in_column(self, left: f64, right: f64) -> Option<Self> {
        let dx = self.b[0] - self.a[0];
        if dx == 0.0 {
            return None;
        }
        let l = (left - self.a[0]) / dx;
        let r = (right - self.a[0]) / dx;
        let start = l.min(r).max(0.0);
        let end = l.max(r).min(1.0);
        (end > start).then(|| Self {
            a: self.at(start),
            b: self.at(end),
        })
    }
}

#[derive(Clone, Copy)]
struct Polygon {
    points: [Point; VERTICES],
    len: usize,
    min: Point,
    max: Point,
}

impl Default for Polygon {
    fn default() -> Self {
        Self {
            points: [[0.0; 2]; VERTICES],
            len: 0,
            min: [f64::INFINITY; 2],
            max: [f64::NEG_INFINITY; 2],
        }
    }
}

impl Polygon {
    fn new(body: DisplacementBody, origin: Point, width: f64) -> Self {
        let mut result = Self::default();
        // Validation has already succeeded. The circle's outline, like its
        // immersion hull, is orientation-independent to avoid artificial wobble.
        let hull = WaterHull::new(body.shape).unwrap();
        let (sin, cos) = if matches!(body.shape, crate::immersion::HullShape::Circle { .. }) {
            (0.0, 1.0)
        } else {
            (body.angle as f64).sin_cos()
        };
        result.len = hull.points().len();
        for (out, p) in result.points.iter_mut().zip(hull.points()) {
            *out = [
                body.center.x as f64 - origin[0] + p[0] * cos - p[1] * sin,
                body.center.y as f64 - origin[1] + p[0] * sin + p[1] * cos,
            ];
        }
        // Common clipping distributes over the union, so clip once here.
        for (axis, bound, greater) in [(0, 0.0, true), (0, width, false), (1, 0.0, true)] {
            clip(&mut result.points, &mut result.len, axis, bound, greater);
        }
        if super::polygon_area(&result.points[..result.len]) == 0.0 {
            result.len = 0;
        }
        for p in &result.points[..result.len] {
            for (axis, value) in p.iter().enumerate() {
                result.min[axis] = result.min[axis].min(*value);
                result.max[axis] = result.max[axis].max(*value);
            }
        }
        result
    }

    fn edges(&self) -> impl Iterator<Item = Edge> + '_ {
        (0..self.len).map(|i| Edge {
            a: self.points[i],
            b: self.points[(i + 1) % self.len],
        })
    }

    fn covers(&self, edge: Edge, owns_coincident: bool) -> Option<[f64; 2]> {
        if self.len == 0
            || (0..2).any(|i| {
                edge.a[i].max(edge.b[i]) < self.min[i] || edge.a[i].min(edge.b[i]) > self.max[i]
            })
        {
            return None;
        }
        let direction = sub(edge.b, edge.a);
        let mut interval: [f64; 2] = [0.0, 1.0];
        for side in self.edges() {
            let d = sub(side.b, side.a);
            let relative = sub(edge.a, side.a);
            let start = cross(d, relative);
            let rate = cross(d, direction);
            let epsilon = 1e-12
                * d[0].hypot(d[1])
                * (1.0 + direction[0].hypot(direction[1]) + relative[0].hypot(relative[1]));
            if rate.abs() <= epsilon {
                if start < -epsilon {
                    return None;
                }
                if start.abs() <= epsilon
                    && d[0] * direction[0] + d[1] * direction[1] > 0.0
                    && !owns_coincident
                {
                    return None;
                }
            } else {
                let t = -start / rate;
                if rate > 0.0 {
                    interval[0] = interval[0].max(t);
                } else {
                    interval[1] = interval[1].min(t);
                }
                if interval[1] <= interval[0] {
                    return None;
                }
            }
        }
        (interval[1] > interval[0]).then_some(interval)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Outline {
    edges: Vec<Edge>,
    top: f64,
    area: f64,
}

impl Outline {
    pub(super) fn rebuild(&mut self, bodies: &[DisplacementBody], spec: &PoolSpec) {
        self.edges.clear();
        self.top = 0.0;
        self.area = 0.0;
        if bodies.is_empty() {
            return;
        }
        if self.edges.capacity() == 0 {
            self.edges.reserve_exact(MAX_SEGMENTS);
        }
        let origin = [spec.left, spec.bed[0]];
        let width = spec.column_width * spec.bed.len() as f64;
        let mut polygons = [Polygon::default(); MAX_DISPLACERS];
        for (polygon, body) in polygons.iter_mut().zip(bodies) {
            *polygon = Polygon::new(*body, origin, width);
            self.top = self.top.max(polygon.max[1]);
        }
        for (i, polygon) in polygons[..bodies.len()].iter().enumerate() {
            for edge in polygon.edges() {
                if edge.a[0] == edge.b[0] {
                    continue; // Vertical edges have zero area integral.
                }
                let mut covered = [[0.0; 2]; MAX_DISPLACERS - 1];
                let mut count = 0;
                for (j, other) in polygons[..bodies.len()].iter().enumerate() {
                    if i != j
                        && let Some(interval) = other.covers(edge, j < i)
                    {
                        covered[count] = interval;
                        count += 1;
                    }
                }
                covered[..count].sort_unstable_by(|a, b| a[0].total_cmp(&b[0]));
                let mut start = 0.0;
                for [lo, hi] in covered[..count].iter().copied() {
                    if lo > start {
                        self.edges.push(Edge {
                            a: edge.at(start),
                            b: edge.at(lo),
                        });
                    }
                    start = start.max(hi);
                }
                if start < 1.0 {
                    self.edges.push(Edge {
                        a: edge.at(start),
                        b: edge.b,
                    });
                }
            }
        }
        debug_assert!(self.edges.len() <= MAX_SEGMENTS);
        self.area = self.area_below(self.top);
    }

    fn area_below(&self, level: f64) -> f64 {
        self.edges
            .iter()
            .filter_map(|e| e.below(level))
            .map(|e| e.integral(level))
            .sum::<f64>()
            .max(0.0)
    }

    pub(super) fn fill_columns(&self, spec: &PoolSpec, liquid: &[f64], occupied: &mut [f64]) {
        let volume: f64 = liquid.iter().sum();
        if volume <= 0.0 || self.area <= 0.0 {
            return;
        }
        let width = spec.column_width * liquid.len() as f64;
        let empty = volume / width; // Heights relative to flat bed.
        let level = if empty >= self.top {
            empty + self.area / width
        } else {
            let mut low = empty;
            let mut high = empty + self.area / width;
            for _ in 0..40 {
                let h = (low + high) * 0.5;
                if width * h - self.area_below(h) < volume {
                    low = h;
                } else {
                    high = h;
                }
            }
            (low + high) * 0.5
        };
        for edge in self.edges.iter().filter_map(|e| e.below(level)) {
            let first = ((edge.a[0].min(edge.b[0]) / spec.column_width)
                .floor()
                .max(0.0) as usize)
                .min(liquid.len());
            let last = ((edge.a[0].max(edge.b[0]) / spec.column_width)
                .ceil()
                .max(0.0) as usize)
                .min(liquid.len());
            for (i, area) in occupied.iter_mut().enumerate().take(last).skip(first) {
                let left = i as f64 * spec.column_width;
                if let Some(segment) = edge.in_column(left, left + spec.column_width) {
                    *area += segment.integral(level);
                }
            }
        }
        for area in occupied {
            *area = area.max(0.0); // Roundoff only; signed edges sum before clamping.
        }
    }
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}
fn cross(a: Point, b: Point) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

#[cfg(test)]
mod tests;
