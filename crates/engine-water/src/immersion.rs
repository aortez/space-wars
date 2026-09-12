//! Allocation-free submerged-area integration. It queries pools only; falling
//! parcels are not a pressure field. A rigid-body adapter supplies pose and COM.
use crate::{WaterError, WaterWorld};
use engine_core::Vec2;

const SIDES: usize = 32;
const CLIPPED_VERTICES: usize = SIDES + 4;
type Point = [f64; 2];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HullShape {
    Box { half_width: f32, half_height: f32 },
    Circle { radius: f32 },
}

/// Geometry in body-origin coordinates. Circles use 32 sides with area weights
/// normalized to pi*r^2, so full submersion gives the correct displaced volume.
pub struct WaterHull {
    points: [Point; SIDES],
    len: usize,
    circle: bool,
    area: f64,
    weight: f64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Immersion {
    pub area: f64,
    /// Integral of displacement from the supplied center of mass, times area.
    pub first_moment: [f64; 2],
    /// Integral of squared distance from that center of mass, times area.
    pub polar_moment: f64,
    /// Area-weighted water velocity and torque about the supplied COM.
    pub flow: [f64; 2],
    pub flow_torque: f64,
    pub wet_columns: usize,
}

impl WaterHull {
    pub fn new(shape: HullShape) -> Result<Self, WaterError> {
        let mut hull = Self {
            points: [[0.0; 2]; SIDES],
            len: 0,
            circle: false,
            area: 0.0,
            weight: 1.0,
        };
        let valid = |v: f32| v.is_finite() && (0.001..=1000.0).contains(&v);
        match shape {
            HullShape::Box {
                half_width,
                half_height,
            } if valid(half_width) && valid(half_height) => {
                let (x, y) = (half_width as f64, half_height as f64);
                hull.points[..4].copy_from_slice(&[[-x, -y], [x, -y], [x, y], [-x, y]]);
                hull.len = 4;
                hull.area = 4.0 * x * y;
            }
            HullShape::Circle { radius } if valid(radius) => {
                for (i, p) in hull.points.iter_mut().enumerate() {
                    let a = std::f64::consts::TAU * i as f64 / SIDES as f64;
                    *p = [radius as f64 * a.cos(), radius as f64 * a.sin()];
                }
                hull.len = SIDES;
                hull.circle = true;
                hull.area = std::f64::consts::PI * (radius as f64).powi(2);
                hull.weight = hull.area / moments(&hull.points).area;
            }
            _ => return Err(WaterError::InvalidGeometry),
        }
        Ok(hull)
    }

    pub fn area(&self) -> f64 {
        self.area
    }

    /// Pools must describe non-overlapping occupied water regions. This models
    /// one hull, not a union of overlapping compound collider parts.
    pub fn measure(
        &self,
        water: &WaterWorld,
        position: Vec2,
        angle: f32,
        center: Vec2,
    ) -> Result<Immersion, WaterError> {
        if ![position.x, position.y, center.x, center.y, angle]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(WaterError::InvalidInput);
        }
        let (sin, cos) = if self.circle {
            (0.0, 1.0)
        } else {
            (angle as f64).sin_cos()
        };
        let mut transformed = [[0.0; 2]; CLIPPED_VERTICES];
        let mut left = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        for (dest, p) in transformed.iter_mut().zip(&self.points[..self.len]) {
            *dest = [
                p[0] * cos - p[1] * sin + (position.x - center.x) as f64,
                p[0] * sin + p[1] * cos + (position.y - center.y) as f64,
            ];
            left = left.min(dest[0] + center.x as f64);
            right = right.max(dest[0] + center.x as f64);
        }
        let mut result = Immersion::default();
        for pool in water.pools() {
            for column in pool.columns_in_range(left, right) {
                if column.volume <= 0.0 {
                    continue;
                }
                let mut polygon = transformed;
                let mut len = self.len;
                for (axis, bound, greater) in [
                    (0, column.left - center.x as f64, true),
                    (0, column.left + column.width - center.x as f64, false),
                    (1, column.bed - center.y as f64, true),
                    (1, column.surface - center.y as f64, false),
                ] {
                    clip(&mut polygon, &mut len, axis, bound, greater);
                }
                let m = moments(&polygon[..len]);
                if m.area <= 0.0 {
                    continue;
                }
                result.area += m.area * self.weight;
                for (sum, value) in result.first_moment.iter_mut().zip(m.first_moment) {
                    *sum += value * self.weight;
                }
                result.polar_moment += m.polar_moment * self.weight;
                result.flow[0] += column.velocity * m.area * self.weight;
                result.flow_torque -= column.velocity * m.first_moment[1] * self.weight;
                result.wet_columns += 1;
            }
        }
        Ok(result)
    }
}

fn clip(
    polygon: &mut [Point; CLIPPED_VERTICES],
    len: &mut usize,
    axis: usize,
    bound: f64,
    greater: bool,
) {
    if *len == 0 {
        return;
    }
    let source = *polygon;
    let mut previous = source[*len - 1];
    let inside = |p: Point| {
        if greater {
            p[axis] >= bound
        } else {
            p[axis] <= bound
        }
    };
    let mut count = 0;
    for &current in &source[..*len] {
        if inside(current) != inside(previous) {
            let t = (bound - previous[axis]) / (current[axis] - previous[axis]);
            polygon[count] = [
                previous[0] + t * (current[0] - previous[0]),
                previous[1] + t * (current[1] - previous[1]),
            ];
            count += 1;
        }
        if inside(current) {
            polygon[count] = current;
            count += 1;
        }
        previous = current;
    }
    *len = count;
}

fn moments(points: &[Point]) -> Immersion {
    let mut m = Immersion::default();
    if points.len() < 3 {
        return m;
    }
    let mut a = points[points.len() - 1];
    for &b in points {
        let cross = a[0] * b[1] - b[0] * a[1];
        m.area += cross * 0.5;
        m.first_moment[0] += (a[0] + b[0]) * cross / 6.0;
        m.first_moment[1] += (a[1] + b[1]) * cross / 6.0;
        m.polar_moment += cross
            * (a[0] * a[0] + a[0] * b[0] + b[0] * b[0] + a[1] * a[1] + a[1] * b[1] + b[1] * b[1])
            / 12.0;
        a = b;
    }
    m
}

#[cfg(test)]
mod tests;
