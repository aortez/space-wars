//! Conservative horizontal remapping for slowly actuated shallow-water floors.
//! This is a quasi-static bed update, not pressure-driven rigid/fluid coupling.
use crate::{
    Boundary, Column, MAX_STEP, Parcel, WaterError, WaterWorld, finite_coordinate, slopes,
};

#[derive(Clone, Copy)]
pub struct PoolGeometry<'a> {
    pub pool: usize,
    pub left: f64,
    pub column_width: f64,
    pub bed_edges: &'a [[f64; 2]],
    pub boundaries: [Boundary; 2],
}

impl PoolGeometry<'_> {
    fn unchanged(self, pool: &crate::Pool) -> bool {
        self.left == pool.spec.left
            && self.column_width == pool.spec.column_width
            && self.boundaries == pool.spec.boundaries
            && pool.slopes.as_deref() == Some(self.bed_edges)
    }
    fn right(self) -> f64 {
        self.left + self.column_width * self.bed_edges.len() as f64
    }
    fn bed_at(self, x: f64) -> f64 {
        let local = ((x - self.left) / self.column_width).clamp(0.0, self.bed_edges.len() as f64);
        let i = (local as usize).min(self.bed_edges.len() - 1);
        let t = local - i as f64;
        self.bed_edges[i][0] * (1.0 - t) + self.bed_edges[i][1] * t
    }
}

impl Column {
    fn area_between(self, left: f64, right: f64) -> f64 {
        if right <= left {
            return 0.0;
        }
        slopes::area(
            [self.bed_at(left), self.bed_at(right)],
            right - left,
            self.surface,
        )
    }

    fn uncovered(self, new: PoolGeometry<'_>) -> [(f64, f64); 2] {
        [
            (self.left, (self.left + self.width).min(new.left)),
            (self.left.max(new.right()), self.left + self.width),
        ]
    }

    fn release(self, left: f64, right: f64, volume: f64) -> Parcel {
        let cut = Self {
            left,
            width: right - left,
            bed_edges: [self.bed_at(left), self.bed_at(right)],
            volume,
            ..self
        };
        Parcel {
            position: cut.water_centroid(),
            velocity: self.flow_velocity(),
            volume,
            duration: 1.0 / 60.0,
            horizontal_bounds: None,
        }
    }
}

impl WaterWorld {
    /// Move already-configured sloping pools together, without changing counts
    /// or allocating. Retained liquid is remapped by old occupied cross-section;
    /// uncovered strips become free parcels, never injection/drainage/reclamation.
    /// Validate the entire batch (including parcel capacity) before any mutation.
    ///
    /// Supported motion is deliberately slow: bed displacement at retained
    /// coordinates cannot exceed gravity*dt²/2 per update. Liquid remains in
    /// quasi-static contact with the gently moving bed. This does NOT support
    /// free-falling floors, pressure/energy coupling, or displacing bodies.
    /// The caller supplies at most one update per simulation step, never paused.
    pub fn move_sloped_pools(
        &mut self,
        changes: &[PoolGeometry<'_>],
        dt: f64,
    ) -> Result<(), WaterError> {
        if !dt.is_finite() || dt <= 0.0 || dt > MAX_STEP {
            return Err(WaterError::InvalidInput);
        }
        let mut releases = 0;
        for (index, &new) in changes.iter().enumerate() {
            let pool = self.pools.get(new.pool).ok_or(WaterError::InvalidInput)?;
            if changes[..index].iter().any(|c| c.pool == new.pool)
                || !pool.enabled
                || pool.slopes.is_none()
                || !pool.displacement.is_empty()
            {
                return Err(WaterError::InvalidInput);
            }
            if new.unchanged(pool) {
                continue;
            }
            if new.bed_edges.len() != pool.volume.len()
                || !finite_coordinate(new.left)
                || !new.column_width.is_finite()
                || new.column_width < 0.001
                || !finite_coordinate(new.right())
                || new
                    .bed_edges
                    .iter()
                    .flatten()
                    .any(|h| !finite_coordinate(*h) || *h <= self.config.exit_y)
            {
                return Err(WaterError::InvalidGeometry);
            }
            for edge in 0..2 {
                let height = new.bed_edges[if edge == 0 {
                    0
                } else {
                    new.bed_edges.len() - 1
                }][edge];
                if let Boundary::Spill { lip } = new.boundaries[edge]
                    && (!finite_coordinate(lip) || lip < height)
                {
                    return Err(WaterError::InvalidGeometry);
                }
                if let Some([left, right]) = pool.outlet_channels[edge]
                    && matches!(new.boundaries[edge], Boundary::Spill { .. })
                    && !(left..=right).contains(&(if edge == 0 { new.left } else { new.right() }))
                {
                    return Err(WaterError::InvalidGeometry);
                }
            }
            let old_right = pool.spec.left + pool.spec.column_width * pool.volume.len() as f64;
            if (new.left - pool.spec.left)
                .abs()
                .max((new.right() - old_right).abs())
                > self.config.max_speed * dt
            {
                return Err(WaterError::InvalidInput);
            }
            // Piecewise linear differences attain their extremes at either
            // profile's vertices. Check BOTH grids, not only old endpoints.
            let max_drop = self.config.gravity * dt * dt * 0.5 + 1e-10;
            for c in pool.columns() {
                for x in [c.left, c.left + c.width] {
                    if x >= new.left
                        && x <= new.right()
                        && (c.bed_at(x) - new.bed_at(x)).abs() > max_drop
                    {
                        return Err(WaterError::InvalidInput);
                    }
                }
                for (left, right) in c.uncovered(new) {
                    releases += usize::from(c.area_between(left, right) > 0.0);
                }
            }
            for (i, endpoints) in new.bed_edges.iter().enumerate() {
                for (edge, &height) in endpoints.iter().enumerate() {
                    let x = new.left + (i + edge) as f64 * new.column_width;
                    if let Some(old) = pool.index(x)
                        && (pool.column(old).bed_at(x) - height).abs() > max_drop
                    {
                        return Err(WaterError::InvalidInput);
                    }
                }
            }
        }
        if releases > self.config.max_parcels - self.parcels.len() {
            return Err(WaterError::Capacity);
        }
        for &new in changes {
            let pool = &mut self.pools[new.pool];
            if new.unchanged(pool) {
                continue;
            }
            pool.geometry_scratch.fill(0.0);
            let mut total = 0.0;
            let mut released = 0.0;
            for i in 0..pool.volume.len() {
                let c = pool.column(i);
                if c.volume == 0.0 {
                    continue;
                }
                total += c.volume;
                for (left, right) in c.uncovered(new) {
                    let volume = c.area_between(left, right);
                    if volume > 0.0 {
                        released += volume;
                        self.parcels.push(c.release(left, right, volume));
                        self.spills.push(None);
                    }
                }
                let left = c.left.max(new.left);
                let right = (c.left + c.width).min(new.right());
                if right <= left {
                    continue;
                }
                let first = ((left - new.left) / new.column_width).floor() as usize;
                let last = (((right - new.left) / new.column_width).ceil() as usize)
                    .min(pool.volume.len());
                for j in first..last {
                    let x = new.left + j as f64 * new.column_width;
                    pool.geometry_scratch[j] +=
                        c.area_between(left.max(x), right.min(x + new.column_width));
                }
            }
            // Normalize rounding in the overlap integrals, not a hidden source
            // or sink. No exact volume is ever rounded down to discard a film.
            let sum: f64 = pool.geometry_scratch.iter().sum();
            if sum > 0.0 {
                let factor = (total - released).max(0.0) / sum;
                for v in &mut pool.geometry_scratch {
                    *v *= factor;
                }
            }
            // Transport the existing horizontal field at world-space faces.
            // No explicit inward force or target velocity is injected here.
            for face in 0..=pool.volume.len() {
                let x = new.left + face as f64 * new.column_width;
                let local = ((x - pool.spec.left) / pool.spec.column_width)
                    .clamp(0.0, pool.volume.len() as f64);
                let i = (local as usize).min(pool.volume.len() - 1);
                let t = local - i as f64;
                pool.flux[face] = pool.velocity[i] * (1.0 - t) + pool.velocity[i + 1] * t;
            }
            std::mem::swap(&mut pool.volume, &mut pool.geometry_scratch);
            std::mem::swap(&mut pool.velocity, &mut pool.flux);
            pool.spec.left = new.left;
            pool.spec.column_width = new.column_width;
            pool.spec.boundaries = new.boundaries;
            pool.slopes.as_mut().unwrap().copy_from_slice(new.bed_edges);
            for (bed, endpoints) in pool.spec.bed.iter_mut().zip(new.bed_edges) {
                *bed = endpoints[0].min(endpoints[1]);
            }
            pool.outlet_credit = [crate::drips::Credit::default(); 2];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
