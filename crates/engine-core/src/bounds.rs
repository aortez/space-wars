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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundsList {
    bounds: Vec<Bounds2>,
}

impl BoundsList {
    pub const fn new() -> Self {
        Self { bounds: Vec::new() }
    }

    pub fn push(&mut self, bounds: Bounds2) {
        match bounds {
            Bounds2::List(list) => self.bounds.extend(list.bounds),
            bounds => self.bounds.push(bounds),
        }
    }

    pub fn extend(&mut self, other: Self) {
        self.bounds.extend(other.bounds);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Bounds2> {
        self.bounds.iter()
    }

    pub fn len(&self) -> usize {
        self.bounds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }

    pub fn intersects(&self, other: &Bounds2) -> bool {
        self.bounds.iter().any(|bounds| bounds.intersects(other))
    }

    fn contains_circle_center(&self, circle: Circle) -> bool {
        self.bounds.iter().any(|bounds| match bounds {
            Bounds2::Circle(existing) => {
                circle.center.distance_to(existing.center) < existing.radius
            }
            Bounds2::List(list) => list.contains_circle_center(circle),
            Bounds2::Line(_) => false,
        })
    }
}

impl Default for BoundsList {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Bounds2 {
    Circle(Circle),
    Line(Line),
    List(BoundsList),
}

impl Bounds2 {
    pub fn intersects(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Circle(a), Self::Circle(b)) => a.intersects_circle(*b),
            (Self::Line(line), Self::Circle(circle)) | (Self::Circle(circle), Self::Line(line)) => {
                line.intersects_circle(*circle)
            }
            (Self::List(list), other) | (other, Self::List(list)) => list.intersects(other),
            (Self::Line(_), Self::Line(_)) => {
                // Line/line intersection is not needed until laser/list bounds.
                false
            }
        }
    }
}

pub fn triangle_low_bound(points: [Vec2; 3]) -> Circle {
    let center = triangle_centroid(points);
    let radius = points
        .iter()
        .map(|point| point.distance_to(center))
        .fold(1.0, f32::max);
    Circle::new(center, radius * 0.99)
}

pub fn triangle_high_bounds(points: [Vec2; 3]) -> BoundsList {
    const CORNER_RADIUS: f32 = 0.1;

    let mut list = BoundsList::new();
    let center = triangle_centroid(points);

    for point in points {
        let corner = point + (center - point).normalized() * CORNER_RADIUS;
        list.push(Bounds2::Circle(Circle::new(corner, CORNER_RADIUS)));
    }

    let mid_side1 = points[0].midpoint(points[1]);
    let mid_side2 = points[0].midpoint(points[2]);
    let mid_side3 = points[1].midpoint(points[2]);
    let avg_distance = 0.333333
        * (mid_side1.distance_to(center)
            + mid_side2.distance_to(center)
            + mid_side3.distance_to(center));
    let min_circle_size = (avg_distance * 0.15).max(2.0);

    add_triangle_bounds_recursive(points[0], points[1], points[2], &mut list, min_circle_size);
    list
}

fn add_triangle_bounds_recursive(
    v1: Vec2,
    v2: Vec2,
    v3: Vec2,
    list: &mut BoundsList,
    min_circle_size: f32,
) {
    let center = triangle_centroid([v1, v2, v3]);
    let p1 = Vec2::closest_point_on_segment(center, v1, v2);
    let p2 = Vec2::closest_point_on_segment(center, v2, v3);
    let p3 = Vec2::closest_point_on_segment(center, v1, v3);
    let d1 = center.distance_to(p1);
    let d2 = center.distance_to(p2);
    let d3 = center.distance_to(p3);
    let radius = if d1 < d2 && d1 < d3 {
        d1
    } else if d2 < d1 && d2 < d3 {
        d2
    } else {
        d3
    };
    let circle = Circle::new(center, radius);

    if !list.contains_circle_center(circle) {
        list.push(Bounds2::Circle(circle));
    }

    let mid_side1 = v1.midpoint(v2);
    let mid_side2 = v2.midpoint(v3);
    let mid_side3 = v1.midpoint(v3);

    if v1.distance_to(center) > min_circle_size {
        add_triangle_bounds_recursive(v1, mid_side1, mid_side3, list, min_circle_size);
    }
    if v2.distance_to(center) > min_circle_size {
        add_triangle_bounds_recursive(v2, mid_side1, mid_side2, list, min_circle_size);
    }
    if v3.distance_to(center) > min_circle_size {
        add_triangle_bounds_recursive(v3, mid_side2, mid_side3, list, min_circle_size);
    }
    if circle.radius > min_circle_size {
        add_triangle_bounds_recursive(mid_side1, mid_side2, mid_side3, list, min_circle_size);
    }
}

fn triangle_centroid(points: [Vec2; 3]) -> Vec2 {
    (points[0] + points[1] + points[2]) * 0.33333334
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

    #[test]
    fn bounds_list_intersects_when_any_nested_bound_intersects() {
        let mut list = BoundsList::new();
        list.push(Bounds2::Circle(Circle::new(Vec2::new(100.0, 0.0), 10.0)));
        list.push(Bounds2::Circle(Circle::new(Vec2::new(0.0, 0.0), 10.0)));

        assert!(
            Bounds2::List(list.clone())
                .intersects(&Bounds2::Circle(Circle::new(Vec2::new(15.0, 0.0), 5.0)))
        );
        assert!(
            !Bounds2::List(list)
                .intersects(&Bounds2::Circle(Circle::new(Vec2::new(30.1, 0.0), 5.0)))
        );
    }

    #[test]
    fn triangle_low_bound_matches_original_sphere_formula() {
        let bounds = triangle_low_bound([
            Vec2::new(0.0, 0.0),
            Vec2::new(6.0, 0.0),
            Vec2::new(0.0, 6.0),
        ]);

        assert_vec_close(bounds.center, Vec2::new(2.0, 2.0));
        assert_close(bounds.radius, 20.0_f32.sqrt() * 0.99);
    }

    #[test]
    fn triangle_high_bounds_seed_corners_and_recurse_deterministically() {
        let points = [
            Vec2::new(0.0, 0.0),
            Vec2::new(6.0, 0.0),
            Vec2::new(0.0, 6.0),
        ];
        let first = triangle_high_bounds(points);
        let replay = triangle_high_bounds(points);
        let circles = first
            .iter()
            .filter_map(|bounds| match bounds {
                Bounds2::Circle(circle) => Some(*circle),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(first, replay);
        assert!(circles.len() > 4);
        assert_vec_close(circles[0].center, Vec2::new(0.07071068, 0.07071068));
        assert_close(circles[0].radius, 0.1);
        assert_eq!(
            circles
                .iter()
                .filter(|circle| circle.center == triangle_centroid(points))
                .count(),
            1
        );
    }
}
