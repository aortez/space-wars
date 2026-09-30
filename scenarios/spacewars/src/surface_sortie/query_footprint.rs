//! Bounded unions of queries actually issued from one coherent source frame.
//! These cover the full measured patch, including failures, not only its route.
use super::*;
use engine_rapier::world::{CollisionGroups, QueryArea};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct QueryFootprint {
    position: Vec2,
    angle: f32,
    /// Ray, capsule and hull unions, respectively. ALL groups deliberately
    /// retain some false rejections instead of combining incompatible filters.
    pub areas: [Option<QueryArea>; 3],
    pub queries: [u32; 3],
    pub complete: bool,
}
impl QueryFootprint {
    pub fn new(position: Vec2, angle: f32) -> Self {
        Self {
            position,
            angle,
            areas: [None; 3],
            queries: [0; 3],
            complete: finite(position) && angle.is_finite(),
        }
    }
    fn local(&self, point: Vec2) -> Vec2 {
        (point - self.position).rotate_radians(-self.angle)
    }
    fn add(&mut self, kind: usize, area: Option<QueryArea>) {
        // A 17-node walk patch has at most 34 node and 384 edge queries;
        // a separate landing measurement has at most 192 total queries.
        if self.queries.iter().sum::<u32>() >= 1024 {
            self.complete = false;
            return;
        }
        self.queries[kind] += 1;
        let Some(area) = area.filter(|a| {
            finite(a.minimum)
                && finite(a.maximum)
                && a.minimum.x <= a.maximum.x
                && a.minimum.y <= a.maximum.y
        }) else {
            self.complete = false;
            return;
        };
        // One fixed margin, never accumulated by repeated unions. It includes
        // transform roundoff and the validator's 0.002-unit motion tolerance.
        let margin = Vec2::new(0.004, 0.004);
        let mut area = QueryArea {
            minimum: area.minimum - margin,
            maximum: area.maximum + margin,
            groups: CollisionGroups::ALL,
        };
        if let Some(old) = self.areas[kind] {
            area.minimum.x = area.minimum.x.min(old.minimum.x);
            area.minimum.y = area.minimum.y.min(old.minimum.y);
            area.maximum.x = area.maximum.x.max(old.maximum.x);
            area.maximum.y = area.maximum.y.max(old.maximum.y);
        }
        self.areas[kind] = Some(area);
    }
    pub fn ray(&mut self, origin: Vec2, direction: Vec2, distance: f32) {
        let area = (finite(origin)
            && finite(direction)
            && direction.length_squared() > f32::EPSILON
            && distance.is_finite()
            && distance >= 0.0)
            .then(|| {
                let a = self.local(origin);
                let b = self.local(origin + direction.normalized() * distance);
                QueryArea {
                    minimum: Vec2::new(a.x.min(b.x), a.y.min(b.y)),
                    maximum: Vec2::new(a.x.max(b.x), a.y.max(b.y)),
                    groups: CollisionGroups::ALL,
                }
            });
        self.add(0, area);
    }
    pub fn capsule(&mut self, position: Vec2, angle: f32, half_segment: f32, radius: f32) {
        let area = (finite(position)
            && angle.is_finite()
            && half_segment.is_finite()
            && half_segment >= 0.0
            && radius.is_finite()
            && radius > 0.0)
            .then(|| {
                let center = self.local(position);
                let axis = Vec2::Y.rotate_radians(angle - self.angle) * half_segment;
                let extent = Vec2::new(axis.x.abs() + radius, axis.y.abs() + radius);
                QueryArea {
                    minimum: center - extent,
                    maximum: center + extent,
                    groups: CollisionGroups::ALL,
                }
            });
        self.add(1, area);
    }
    pub fn hull(&mut self, area: Option<QueryArea>) {
        self.add(2, area);
    }
}
fn finite(p: Vec2) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotated_rays_and_capsules_are_bounded_and_invalid_or_overflow_capture_is_incomplete() {
        let anchor = Vec2::new(5.0, -2.0);
        let angle = 0.7;
        let mut f = QueryFootprint::new(anchor, angle);
        let origin = anchor + Vec2::new(2.0, 3.0).rotate_radians(angle);
        let direction = Vec2::new(0.0, -10.0).rotate_radians(angle);
        f.ray(origin, direction, 5.0);
        let area = f.areas[0].unwrap();
        assert!((area.minimum.x - 1.996).abs() < 0.00001);
        assert!((area.minimum.y + 2.004).abs() < 0.00001);
        assert!((area.maximum.y - 3.004).abs() < 0.00001);
        f.capsule(origin, angle + std::f32::consts::FRAC_PI_2, 2.0, 0.3);
        let area = f.areas[1].unwrap();
        assert!(area.minimum.x < -0.3 && area.maximum.x > 4.3);
        assert!(area.minimum.y < 2.7 && area.maximum.y > 3.3);
        assert!(f.complete);
        let mut invalid = f.clone();
        invalid.ray(origin, Vec2::ZERO, 2.0);
        assert!(!invalid.complete);
        for _ in 0..1024 {
            f.ray(origin, direction, 5.0);
        }
        assert!(!f.complete);
        assert_eq!(f.queries.iter().sum::<u32>(), 1024);
        assert_eq!(f.areas.into_iter().flatten().count(), 2);
    }
}
