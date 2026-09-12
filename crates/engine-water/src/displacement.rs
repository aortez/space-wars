//! Conservative, hydrostatic-reference displacement for bounded solid unions
//! in closed flat basins. Occupancy raises local pressure heads; existing
//! level-driven fluxes spread the disturbance without adding/removing liquid.
//! This is not a solid flow barrier or a general moving-boundary fluid solver.
use crate::{
    Boundary, Pool, WaterError, WaterWorld,
    immersion::{HullShape, clip},
};
use engine_core::Vec2;

mod union;

/// Per pool, not a general large-body-count fluid solver.
pub const MAX_DISPLACERS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplacementBody {
    pub center: Vec2,
    /// Counterclockwise radians. Circle orientation has no geometric effect.
    pub angle: f32,
    pub shape: HullShape,
}

impl DisplacementBody {
    fn as_box(self) -> Option<DisplacementBox> {
        match self.shape {
            HullShape::Box {
                half_width,
                half_height,
            } => Some(DisplacementBox {
                center: self.center,
                half_extents: Vec2::new(half_width, half_height),
                angle: self.angle,
            }),
            HullShape::Circle { .. } => None,
        }
    }

    fn contains(self, point: Vec2) -> bool {
        match self.shape {
            HullShape::Box { .. } => self.as_box().unwrap().contains(point),
            HullShape::Circle { radius } => {
                let x = point.x as f64 - self.center.x as f64;
                let y = point.y as f64 - self.center.y as f64;
                x * x + y * y <= (radius as f64).powi(2)
            }
        }
    }
}

impl From<DisplacementBox> for DisplacementBody {
    fn from(value: DisplacementBox) -> Self {
        Self {
            center: value.center,
            angle: value.angle,
            shape: HullShape::Box {
                half_width: value.half_extents.x,
                half_height: value.half_extents.y,
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Displacement {
    bodies: Vec<DisplacementBody>,
    outline: union::Outline,
}

impl Displacement {
    pub(crate) fn contains(&self, point: Vec2) -> bool {
        self.bodies.iter().any(|body| body.contains(point))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplacementBox {
    pub center: Vec2,
    pub half_extents: Vec2,
    /// Counterclockwise radians, matching the body's authoritative pose.
    pub angle: f32,
}

impl DisplacementBox {
    pub(crate) fn contains(self, point: Vec2) -> bool {
        let (sin, cos) = (self.angle as f64).sin_cos();
        let x = point.x as f64 - self.center.x as f64;
        let y = point.y as f64 - self.center.y as f64;
        (x * cos + y * sin).abs() <= self.half_extents.x as f64
            && (-x * sin + y * cos).abs() <= self.half_extents.y as f64
    }
}

impl WaterWorld {
    /// Replace the complete occupancy input for this pool, or remove it with
    /// None. One-way mode is simply no occupancy input. Rejections are atomic.
    /// Supported: closed flat basins, box diagonal <= 75% of basin width. The
    /// orientation-independent bound keeps free capacity positive at every
    /// angle and prevents an accepted body rotating into an unsupported pose.
    pub fn set_displacer(
        &mut self,
        pool: usize,
        value: Option<DisplacementBox>,
    ) -> Result<(), WaterError> {
        match value {
            Some(body) => self.set_displacers(pool, &[body.into()]),
            None => self.set_displacers(pool, &[]),
        }
    }

    /// Atomically replace this pool's complete occupancy snapshot. An empty
    /// slice clears it; omitted bodies are removed, never retained implicitly.
    /// Submit once per pool, not separately for each body. Call before stepping
    /// water and again after mechanics to align rendering with the final poses.
    ///
    /// Up to MAX_DISPLACERS boxes/circles, in closed flat basins. The SUM of box
    /// diagonals/circle diameters must fit within 75% of basin width, including
    /// bodies currently dry or outside it. This conservative, pose-independent
    /// bound guarantees positive free capacity even as bodies move/rotate.
    /// Overlapping outlines count only once. Circles use inscribed 32-gons
    /// (full area underestimation < 0.65%); no overlap-dependent area weighting.
    /// Scratch storage is allocated on first use and reused by later snapshots.
    pub fn set_displacers(
        &mut self,
        pool: usize,
        bodies: &[DisplacementBody],
    ) -> Result<(), WaterError> {
        let pool = self.pools.get_mut(pool).ok_or(WaterError::InvalidInput)?;
        if bodies.len() > MAX_DISPLACERS {
            return Err(WaterError::Capacity);
        }
        let mut diameter = 0.0;
        for body in bodies {
            let valid_size = |size: f32| size.is_finite() && (0.001..=1000.0).contains(&size);
            let valid_shape = match body.shape {
                HullShape::Box {
                    half_width,
                    half_height,
                } => valid_size(half_width) && valid_size(half_height),
                HullShape::Circle { radius } => valid_size(radius),
            };
            if ![body.center.x, body.center.y]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1.0e6)
                || !body.angle.is_finite()
                || !valid_shape
            {
                return Err(WaterError::InvalidInput);
            }
            diameter += match body.shape {
                HullShape::Box {
                    half_width,
                    half_height,
                } => 2.0 * (half_width as f64).hypot(half_height as f64),
                HullShape::Circle { radius } => 2.0 * radius as f64,
            };
        }
        if diameter > pool.spec.column_width * pool.volume.len() as f64 * 0.75 {
            return Err(WaterError::InvalidInput);
        }
        if !bodies.is_empty()
            && (pool.spec.boundaries != [Boundary::Closed; 2]
                || pool.spec.bed.iter().any(|bed| *bed != pool.spec.bed[0]))
        {
            return Err(WaterError::InvalidGeometry);
        }
        if pool.displacement.bodies != bodies {
            if pool.displacement.bodies.capacity() == 0 {
                pool.displacement.bodies.reserve_exact(MAX_DISPLACERS);
            }
            pool.displacement.bodies.clear();
            pool.displacement.bodies.extend_from_slice(bodies);
            if bodies.len() != 1 || bodies[0].as_box().is_none() {
                pool.displacement.outline.rebuild(bodies, &pool.spec);
            }
        }
        pool.displaced.fill(0.0);
        pool.refresh_displacement();
        Ok(())
    }
}

impl Pool {
    pub(crate) fn refresh_displacement(&mut self) {
        if self.displacement.bodies.is_empty() {
            return;
        }
        self.displaced.fill(0.0);
        let body = if self.displacement.bodies.len() == 1 {
            self.displacement.bodies[0].as_box()
        } else {
            None
        };
        let Some(body) = body else {
            self.displacement
                .outline
                .fill_columns(&self.spec, &self.volume, &mut self.displaced);
            return;
        };
        if body.angle != 0.0 {
            self.refresh_rotated_displacement(body);
            return;
        }
        // Preserve the analytic axis-aligned path (and existing control runs).
        let width = self.spec.column_width * self.volume.len() as f64;
        let bed = self.spec.bed[0];
        let left = (body.center.x as f64 - body.half_extents.x as f64).max(self.spec.left);
        let right = (body.center.x as f64 + body.half_extents.x as f64).min(self.spec.left + width);
        let bottom = (body.center.y as f64 - body.half_extents.y as f64).max(bed);
        let top = body.center.y as f64 + body.half_extents.y as f64;
        if right <= left || top <= bottom {
            return;
        }
        let occupied_width = right - left;
        let liquid: f64 = self.volume.iter().sum();
        let unoccupied_level = bed + liquid / width;
        if unoccupied_level <= bottom {
            return;
        }
        let below_top = width * (top - bed) - occupied_width * (top - bottom);
        let level = if liquid >= below_top {
            bed + (liquid + occupied_width * (top - bottom)) / width
        } else {
            // Invert free capacity through the partly submerged box. The
            // footprint restriction leaves a positive free basin cross-section.
            (liquid + width * bed - occupied_width * bottom) / (width - occupied_width)
        };
        let depth = (level - bottom).clamp(0.0, top - bottom);
        for (i, area) in self.displaced.iter_mut().enumerate() {
            let x = self.spec.left + i as f64 * self.spec.column_width;
            *area = (right.min(x + self.spec.column_width) - left.max(x)).max(0.0) * depth;
        }
    }

    fn refresh_rotated_displacement(&mut self, body: DisplacementBox) {
        // Work relative to the body origin, avoiding cancellation in polygon
        // areas when a small hull has large world coordinates. A rectangle
        // clipped against the tank/water/column bounds needs at most 8 vertices.
        let (sin, cos) = (body.angle as f64).sin_cos();
        let (x, y) = (body.half_extents.x as f64, body.half_extents.y as f64);
        let mut polygon = [[0.0; 2]; 12];
        for (p, [x, y]) in polygon.iter_mut().zip([[-x, -y], [x, -y], [x, y], [-x, y]]) {
            *p = [x * cos - y * sin, x * sin + y * cos];
        }
        let mut len = 4;
        let width = self.spec.column_width * self.volume.len() as f64;
        let left = self.spec.left - body.center.x as f64;
        let bed = self.spec.bed[0] - body.center.y as f64;
        for (axis, bound, greater) in [(0, left, true), (0, left + width, false), (1, bed, true)] {
            clip(&mut polygon, &mut len, axis, bound, greater);
        }
        let area = polygon_area(&polygon[..len]);
        let liquid: f64 = self.volume.iter().sum();
        if area == 0.0 || liquid == 0.0 {
            return;
        }
        let empty_level = bed + liquid / width;
        let bottom = polygon[..len]
            .iter()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min);
        let top = polygon[..len]
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max);
        if empty_level <= bottom {
            return;
        }
        if empty_level < top {
            // Solve W*(h-bed) - occupied(h) = liquid. Free width is always
            // >= W/4, so capacity is monotone. Fixed iterations bound work;
            // this solves the reference surface, not each rippling column.
            let mut low = empty_level;
            let mut high = empty_level + area / width;
            for _ in 0..40 {
                let level = (low + high) * 0.5;
                let mut submerged = polygon;
                let mut count = len;
                clip(&mut submerged, &mut count, 1, level, false);
                if width * (level - bed) - polygon_area(&submerged[..count]) < liquid {
                    low = level;
                } else {
                    high = level;
                }
            }
            clip(&mut polygon, &mut len, 1, (low + high) * 0.5, false);
        }
        let min_x = polygon[..len]
            .iter()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min);
        let max_x = polygon[..len]
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        for (i, occupied) in self.displaced.iter_mut().enumerate() {
            let x = left + i as f64 * self.spec.column_width;
            if x >= max_x || x + self.spec.column_width <= min_x {
                continue;
            }
            let mut column = polygon;
            let mut count = len;
            clip(&mut column, &mut count, 0, x, true);
            clip(
                &mut column,
                &mut count,
                0,
                x + self.spec.column_width,
                false,
            );
            *occupied = polygon_area(&column[..count]);
        }
    }
}

fn polygon_area(points: &[[f64; 2]]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let origin = points[0];
    points[1..]
        .windows(2)
        .map(|pair| {
            let a = [pair[0][0] - origin[0], pair[0][1] - origin[1]];
            let b = [pair[1][0] - origin[0], pair[1][1] - origin[1]];
            (a[0] * b[1] - b[0] * a[1]) * 0.5
        })
        .sum::<f64>()
        .max(0.0)
}

#[cfg(test)]
mod tests;
