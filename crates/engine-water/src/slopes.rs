//! Opt-in fixed, piecewise-linear beds under the existing fixed-down water.
//! A bed profile is construction-time geometry, NOT a moving-floor API.
use crate::{Boundary, Column, Pool, WaterConfig, WaterError, WaterWorld, finite_coordinate};
use engine_core::Vec2;

/// Water area above a straight bed, including a partially wet triangular cell.
pub(crate) fn area(bed: [f64; 2], width: f64, height: f64) -> f64 {
    let low = bed[0].min(bed[1]);
    let high = bed[0].max(bed[1]);
    let depth = (height - low).max(0.0);
    if height >= high || high == low {
        width * (height - (low + high) * 0.5).max(0.0)
    } else {
        width * depth * depth / (2.0 * (high - low))
    }
}

pub(crate) fn level(bed: [f64; 2], width: f64, volume: f64) -> f64 {
    let low = bed[0].min(bed[1]);
    let high = bed[0].max(bed[1]);
    let depth = volume / width;
    if depth >= (high - low) * 0.5 {
        (low + high) * 0.5 + depth
    } else {
        low + (2.0 * depth * (high - low)).sqrt()
    }
}

impl WaterWorld {
    /// Set a fixed bed's left/right height in each existing column, before the
    /// first step and before this pool contains liquid/displacing bodies.
    /// Adjacent heights may differ: a true ledge stays a ledge, not a ramp.
    /// The x grid, budgets and water accounting do not change. Rejection is atomic.
    /// Moving a wet bed requires a separate conservative transfer operation;
    /// callers must not use this to teleport attached water with terrain.
    pub fn configure_sloped_bed(
        &mut self,
        index: usize,
        edges: &[[f64; 2]],
    ) -> Result<(), WaterError> {
        let pool = self.pools.get(index).ok_or(WaterError::InvalidInput)?;
        if self.tick != 0 || pool.volume.iter().any(|v| *v != 0.0) || !pool.displacement.is_empty()
        {
            return Err(WaterError::InvalidInput);
        }
        if edges.len() != pool.volume.len()
            || edges
                .iter()
                .flatten()
                .any(|h| !finite_coordinate(*h) || *h <= self.config.exit_y)
        {
            return Err(WaterError::InvalidGeometry);
        }
        for edge in 0..2 {
            let height = edges[if edge == 0 { 0 } else { edges.len() - 1 }][edge];
            if let Boundary::Spill { lip } = pool.spec.boundaries[edge]
                && lip < height
            {
                return Err(WaterError::InvalidGeometry);
            }
        }
        let pool = &mut self.pools[index];
        for (height, endpoints) in pool.spec.bed.iter_mut().zip(edges) {
            *height = endpoints[0].min(endpoints[1]);
        }
        pool.slopes = Some(edges.to_vec());
        pool.geometry_scratch.resize(edges.len(), 0.0);
        Ok(())
    }
}

impl Pool {
    pub(crate) fn bed_edges(&self, i: usize) -> [f64; 2] {
        self.slopes.as_ref().map_or([self.spec.bed[i]; 2], |b| b[i])
    }

    pub(crate) fn face_bed(&self, face: usize) -> f64 {
        self.bed_edges(face - 1)[1].max(self.bed_edges(face)[0])
    }

    pub(crate) fn slope_flux(&mut self, config: WaterConfig, dt: f64, limit: f64) {
        let dx = self.spec.column_width;
        for face in 1..self.volume.len() {
            let left = self.surface(face - 1);
            let right = self.surface(face);
            let next = ((self.velocity[face] + config.gravity * (left - right) / dx * dt)
                / (1.0 + config.damping * dt))
                .clamp(-limit, limit);
            let depth = ((if next > 0.0 { left } else { right }) - self.face_bed(face)).max(0.0);
            self.velocity[face] = if depth > 0.0 { next } else { 0.0 };
            self.flux[face] = self.velocity[face] * depth * dt;
        }
    }

    pub(crate) fn outlet_velocity(&self, edge: usize, speed: f64, limit: f64) -> Vec2 {
        let i = if edge == 0 { 0 } else { self.volume.len() - 1 };
        let slope = self.column(i).bed_slope();
        let speed = speed.clamp(-limit / slope.hypot(1.0), limit / slope.hypot(1.0));
        Vec2::new(speed as f32, (speed * slope) as f32)
    }
}

impl Column {
    pub fn bed_slope(self) -> f64 {
        (self.bed_edges[1] - self.bed_edges[0]) / self.width
    }

    pub fn bed_at(self, x: f64) -> f64 {
        self.bed_edges[0] + (x - self.left) * self.bed_slope()
    }

    /// Horizontal area occupied below a proposed level, not an injected amount.
    pub fn area_below(self, height: f64) -> f64 {
        area(self.bed_edges, self.width, height)
    }

    /// Shallow flow follows the bed tangent; this is not a vertical pressure field.
    pub fn flow_velocity(self) -> Vec2 {
        Vec2::new(
            self.velocity as f32,
            (self.velocity * self.bed_slope()) as f32,
        )
    }

    pub fn wet_interval(self) -> [f64; 2] {
        let [a, b] = self.bed_edges;
        let mut span = [self.left, self.left + self.width];
        if self.surface < a.max(b) && a != b {
            let crossing = self.left + self.width * ((self.surface - a) / (b - a)).clamp(0.0, 1.0);
            span[usize::from(b > a)] = crossing;
        }
        span
    }

    /// Centroid of the occupied trapezoid/triangle; safe for support release.
    pub fn water_centroid(self) -> Vec2 {
        let [left, right] = self.wet_interval();
        let a = (self.surface - self.bed_at(left)).max(0.0);
        let b = (self.surface - self.bed_at(right)).max(0.0);
        if a + b == 0.0 {
            return Vec2::new(((left + right) * 0.5) as f32, self.surface as f32);
        }
        Vec2::new(
            (left + (right - left) * (a + 2.0 * b) / (3.0 * (a + b))) as f32,
            (self.surface - (a * a + a * b + b * b) / (3.0 * (a + b))) as f32,
        )
    }

    /// Top-side arrival on either wet surface or exposed sloping bed. A side
    /// entry into occupied water also counts; entry from below terrain does not.
    pub(crate) fn swept_entry(self, from: Vec2, to: Vec2) -> Option<f64> {
        let x = from.x as f64;
        let y = from.y as f64;
        let dx = (to.x - from.x) as f64;
        let dy = (to.y - from.y) as f64;
        let (start, end) = if dx == 0.0 {
            if x < self.left - 1e-8 || x > self.left + self.width + 1e-8 {
                return None;
            }
            (0.0, 1.0)
        } else {
            let a = (self.left - x) / dx;
            let b = (self.left + self.width - x) / dx;
            (a.min(b).max(0.0), a.max(b).min(1.0))
        };
        if start > end {
            return None;
        }
        let at_start = y + dy * start;
        let bed_start = self.bed_at(x + dx * start);
        if at_start < bed_start - 1e-8 {
            return None;
        }
        if at_start <= self.surface && at_start >= bed_start {
            return Some(start);
        }
        let mut earliest = None;
        if dy < 0.0 {
            let t = (self.surface - y) / dy;
            if t >= start && t <= end && self.surface >= self.bed_at(x + dx * t) - 1e-8 {
                earliest = Some(t);
            }
        }
        let relative = dy - self.bed_slope() * dx;
        if relative < 0.0 {
            let t = (self.bed_at(x) - y) / relative;
            if t >= start && t <= end && earliest.is_none_or(|old| t < old) {
                earliest = Some(t);
            }
        }
        earliest
    }
}

#[cfg(test)]
mod tests;
