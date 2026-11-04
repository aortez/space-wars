//! Parameter-space optical clipping against the canonical admitted shapes.
//! Keep geometry and endpoint trims in f64 until the final diagnostic distance;
//! adding 0.001 to a world-space f32, or subtracting it from a long f32 ray, loses it.
use super::*;
use rapier2d::parry::shape::Shape;

#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}
impl From<Vector> for Point {
    fn from(p: Vector) -> Self {
        Self {
            x: p.x as f64,
            y: p.y as f64,
        }
    }
}
impl Point {
    fn sub(self, p: Self) -> Self {
        Self {
            x: self.x - p.x,
            y: self.y - p.y,
        }
    }
    fn dot(self, p: Self) -> f64 {
        self.x * p.x + self.y * p.y
    }
    fn cross(self, p: Self) -> f64 {
        self.x * p.y - self.y * p.x
    }
    fn local(self, pose: &Pose) -> Self {
        let p = self.sub(pose.translation.into());
        let axis: Point = (pose.rotation * Vector::X).into();
        let norm = axis.dot(axis);
        Self {
            x: p.dot(axis) / norm,
            y: (axis.x * p.y - axis.y * p.x) / norm,
        }
    }
}
#[derive(Clone, Copy)]
struct Segment {
    a: Point,
    b: Point,
}
impl Segment {
    fn local(self, pose: &Pose) -> Self {
        Self {
            a: self.a.local(pose),
            b: self.b.local(pose),
        }
    }
    fn delta(self) -> Point {
        self.b.sub(self.a)
    }
}

pub(super) fn cast(ray: LightRay, id: LightId, shape: &dyn Shape, pose: &Pose) -> Option<f32> {
    let line = Segment {
        a: to_rapier(ray.receiver.position).into(),
        b: to_rapier(ray.endpoint.position).into(),
    };
    let length = line.delta().dot(line.delta()).sqrt();
    if !length.is_finite() || length == 0.0 {
        return None;
    }
    let trim = (ENDPOINT_TOLERANCE as f64 / length).min(0.25);
    let start = if ray.receiver.surface == Some(id) {
        trim
    } else {
        0.0
    };
    let end = if ray.endpoint.surface == Some(id) {
        1.0 - trim
    } else {
        1.0
    };
    clip(shape, line.local(pose), start, end).map(|t| (t * length) as f32)
}

fn interval(enter: f64, exit: f64, low: f64, high: f64) -> Option<f64> {
    let first = enter.max(low);
    (first <= exit.min(high)).then_some(first)
}
fn ball(line: Segment, center: Point, radius: f64, low: f64, high: f64) -> Option<f64> {
    let delta = line.delta();
    let offset = line.a.sub(center);
    let squared_length = delta.dot(delta);
    // Perpendicular distance avoids subtracting two nearly equal O(length^4)
    // terms for a small emitter/blocker at the far end of a long segment.
    let perpendicular = offset.cross(delta);
    let radial = radius * radius - perpendicular * perpendicular / squared_length;
    if radial < 0.0 {
        return None;
    }
    let midpoint = -offset.dot(delta) / squared_length;
    let half = (radial / squared_length).sqrt();
    interval(midpoint - half, midpoint + half, low, high)
}
fn rectangle(line: Segment, min: Point, max: Point, mut low: f64, mut high: f64) -> Option<f64> {
    let delta = line.delta();
    for (origin, direction, min, max) in [
        (line.a.x, delta.x, min.x, max.x),
        (line.a.y, delta.y, min.y, max.y),
    ] {
        if direction == 0.0 {
            if origin < min || origin > max {
                return None;
            }
        } else {
            let a = (min - origin) / direction;
            let b = (max - origin) / direction;
            low = low.max(a.min(b));
            high = high.min(a.max(b));
        }
    }
    (low <= high).then_some(low)
}
fn edge(line: Segment, a: Point, b: Point, low: f64, high: f64) -> Option<f64> {
    let direction = line.delta();
    let side = b.sub(a);
    let offset = a.sub(line.a);
    let denominator = direction.cross(side);
    if denominator == 0.0 {
        if offset.cross(direction) != 0.0 {
            return None;
        }
        let (t0, t1) = if direction.x.abs() >= direction.y.abs() {
            (
                (a.x - line.a.x) / direction.x,
                (b.x - line.a.x) / direction.x,
            )
        } else {
            (
                (a.y - line.a.y) / direction.y,
                (b.y - line.a.y) / direction.y,
            )
        };
        return interval(t0.min(t1), t0.max(t1), low, high);
    }
    let t = offset.cross(side) / denominator;
    let u = offset.cross(direction) / denominator;
    (t >= low && t <= high && (0.0..=1.0).contains(&u)).then_some(t)
}
fn clip(shape: &dyn Shape, line: Segment, mut low: f64, mut high: f64) -> Option<f64> {
    if let Some(shape) = shape.as_compound() {
        return shape
            .shapes()
            .iter()
            .filter_map(|(pose, child)| clip(child.as_ref(), line.local(pose), low, high))
            .min_by(f64::total_cmp);
    }
    if let Some(shape) = shape.as_ball() {
        return ball(
            line,
            Point { x: 0.0, y: 0.0 },
            shape.radius as f64,
            low,
            high,
        );
    }
    if let Some(shape) = shape.as_cuboid() {
        let half: Point = shape.half_extents.into();
        return rectangle(
            line,
            Point {
                x: -half.x,
                y: -half.y,
            },
            half,
            low,
            high,
        );
    }
    if let Some(shape) = shape.as_capsule() {
        let a: Point = shape.segment.a.into();
        let b: Point = shape.segment.b.into();
        let axis = b.sub(a);
        let length = axis.dot(axis).sqrt();
        let radius = shape.radius as f64;
        if length == 0.0 {
            return ball(line, a, radius, low, high);
        }
        let convert = |p: Point| {
            let p = p.sub(a);
            Point {
                x: p.dot(axis) / length,
                y: axis.cross(p) / length,
            }
        };
        return [
            ball(line, a, radius, low, high),
            ball(line, b, radius, low, high),
            rectangle(
                Segment {
                    a: convert(line.a),
                    b: convert(line.b),
                },
                Point { x: 0.0, y: -radius },
                Point {
                    x: length,
                    y: radius,
                },
                low,
                high,
            ),
        ]
        .into_iter()
        .flatten()
        .min_by(f64::total_cmp);
    }
    if let Some(shape) = shape.as_convex_polygon() {
        for (point, normal) in shape.points().iter().zip(shape.normals()) {
            let normal: Point = (*normal).into();
            let numerator = Point::from(*point).sub(line.a).dot(normal);
            let denominator = line.delta().dot(normal);
            if denominator == 0.0 {
                if numerator < 0.0 {
                    return None;
                }
            } else if denominator > 0.0 {
                high = high.min(numerator / denominator);
            } else {
                low = low.max(numerator / denominator);
            }
        }
        return (low <= high).then_some(low);
    }
    if let Some(shape) = shape.as_polyline() {
        return shape
            .indices()
            .iter()
            .filter_map(|indices| {
                edge(
                    line,
                    shape.vertices()[indices[0] as usize].into(),
                    shape.vertices()[indices[1] as usize].into(),
                    low,
                    high,
                )
            })
            .min_by(f64::total_cmp);
    }
    unreachable!("lighting admission rejects unsupported canonical shapes")
}
