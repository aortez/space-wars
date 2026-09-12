//! Conservative, hydrostatic-reference displacement for one axis-aligned box
//! per closed flat basin. Occupancy raises local pressure heads; existing
//! level-driven fluxes spread the disturbance without adding/removing liquid.
//! This is not a solid flow barrier or a general moving-boundary fluid solver.
use crate::{Boundary, Pool, WaterError, WaterWorld};
use engine_core::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplacementBox {
    pub center: Vec2,
    pub half_extents: Vec2,
}

impl WaterWorld {
    /// Replace the complete occupancy input for this pool, or remove it with
    /// None. One-way mode is simply no occupancy input. Rejections are atomic.
    /// Supported: closed flat basins, box width <= 75% of basin width. Boxes can
    /// move horizontally/vertically; rotation and multiple boxes are later work.
    pub fn set_displacer(
        &mut self,
        pool: usize,
        value: Option<DisplacementBox>,
    ) -> Result<(), WaterError> {
        let pool = self.pools.get_mut(pool).ok_or(WaterError::InvalidInput)?;
        if let Some(body) = value {
            if ![body.center.x, body.center.y]
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1.0e6)
                || ![body.half_extents.x, body.half_extents.y]
                    .iter()
                    .all(|v| v.is_finite() && (0.001..=1000.0).contains(v))
                || body.half_extents.x as f64 * 2.0
                    > pool.spec.column_width * pool.volume.len() as f64 * 0.75
            {
                return Err(WaterError::InvalidInput);
            }
            if pool.spec.boundaries != [Boundary::Closed; 2]
                || pool.spec.bed.iter().any(|bed| *bed != pool.spec.bed[0])
            {
                return Err(WaterError::InvalidGeometry);
            }
        }
        pool.displacer = value;
        pool.displaced.fill(0.0);
        pool.refresh_displacement();
        Ok(())
    }
}

impl Pool {
    pub(crate) fn refresh_displacement(&mut self) {
        let Some(body) = self.displacer else {
            return;
        };
        self.displaced.fill(0.0);
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
}

#[cfg(test)]
mod tests;
