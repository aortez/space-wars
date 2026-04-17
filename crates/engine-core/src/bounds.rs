//! Basic 2D bounds and intersection helpers.

use serde::{Deserialize, Serialize};

use crate::math::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Circle {
    pub center: Vec2,
    pub radius: f32,
}

impl Circle {
    pub const fn new(center: Vec2, radius: f32) -> Self {
        Self { center, radius }
    }

    pub fn intersects_circle(self, other: Self) -> bool {
        self.center.distance_to(other.center) <= self.radius + other.radius
    }

    pub fn contains_circle(self, other: Self) -> Option<Vec2> {
        let offset = other.center - self.center;
        let distance = offset.length();
        let overlap = distance + other.radius - self.radius;
        if overlap <= 0.0 {
            None
        } else if distance == 0.0 {
            Some(Vec2::X * -overlap)
        } else {
            Some(offset.normalized() * -overlap)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub start: Vec2,
    pub end: Vec2,
}

impl Line {
    pub const fn new(start: Vec2, end: Vec2) -> Self {
        Self { start, end }
    }

    pub fn length(self) -> f32 {
        self.start.distance_to(self.end)
    }

    pub fn center(self) -> Vec2 {
        self.start.midpoint(self.end)
    }

    pub fn intersects_circle(self, circle: Circle) -> bool {
        let closest = Vec2::closest_point_on_segment(circle.center, self.start, self.end);
        closest.distance_to(circle.center) < circle.radius
    }

    pub fn nearest_circle_intersection(self, circle: Circle) -> Option<Vec2> {
        let delta = self.start - self.end;
        let a = delta.length_squared();
        let b = 2.0
            * (delta.x * (self.start.x - circle.center.x)
                + delta.y * (self.start.y - circle.center.y));
        let c = (self.start - circle.center).length_squared() - circle.radius * circle.radius;
        let det = b * b - 4.0 * a * c;

        if a <= 1.0e-6 || det < 0.0 {
            return None;
        }

        let intersection_at =
            |t: f32| Vec2::new(self.start.x + t * delta.x, self.start.y + t * delta.y);

        if det == 0.0 {
            return Some(intersection_at(-b / (2.0 * a)));
        }

        let sqrt_det = det.sqrt();
        let first = intersection_at((-b + sqrt_det) / (2.0 * a));
        let second = intersection_at((-b - sqrt_det) / (2.0 * a));
        if first.distance_to(self.start) < second.distance_to(self.start) {
            Some(first)
        } else {
            Some(second)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Bounds2 {
    Circle(Circle),
    Line(Line),
}

impl Bounds2 {
    pub fn intersects(self, other: Self) -> bool {
        match (self, other) {
            (Self::Circle(a), Self::Circle(b)) => a.intersects_circle(b),
            (Self::Line(line), Self::Circle(circle)) | (Self::Circle(circle), Self::Line(line)) => {
                line.intersects_circle(circle)
            }
            (Self::Line(_), Self::Line(_)) => {
                // Line/line intersection is not needed until laser/list bounds.
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1.0e-5;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= EPS,
            "actual {actual} expected {expected}"
        );
    }

    fn assert_vec_close(actual: Vec2, expected: Vec2) {
        assert_close(actual.x, expected.x);
        assert_close(actual.y, expected.y);
    }

    #[test]
    fn circle_circle_intersection_touches_at_sum_of_radii() {
        let a = Circle::new(Vec2::new(0.0, 0.0), 10.0);
        let b = Circle::new(Vec2::new(15.0, 0.0), 5.0);
        let c = Circle::new(Vec2::new(15.1, 0.0), 5.0);

        assert!(a.intersects_circle(b));
        assert!(!a.intersects_circle(c));
    }

    #[test]
    fn contains_circle_returns_correction_vector_when_outside() {
        let outer = Circle::new(Vec2::new(0.0, 0.0), 10.0);
        let inner = Circle::new(Vec2::new(8.0, 0.0), 3.0);

        assert_vec_close(outer.contains_circle(inner).unwrap(), Vec2::new(-1.0, 0.0));
        assert_eq!(
            outer.contains_circle(Circle::new(Vec2::new(0.0, 0.0), 3.0)),
            None
        );
    }

    #[test]
    fn line_circle_intersection_uses_java_nearest_to_start_choice() {
        let line = Line::new(Vec2::new(-10.0, 0.0), Vec2::new(10.0, 0.0));
        let circle = Circle::new(Vec2::ZERO, 5.0);

        assert!(line.intersects_circle(circle));
        assert_vec_close(
            line.nearest_circle_intersection(circle).unwrap(),
            Vec2::new(-5.0, 0.0),
        );
    }

    #[test]
    fn line_circle_miss_returns_none() {
        let line = Line::new(Vec2::new(-10.0, 6.0), Vec2::new(10.0, 6.0));
        let circle = Circle::new(Vec2::ZERO, 5.0);

        assert!(!line.intersects_circle(circle));
        assert_eq!(line.nearest_circle_intersection(circle), None);
    }
}
