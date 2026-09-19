//! Fixed geometry with switchable supports (e.g. disappearing platforms).
//! No moving-solid collision or dynamic grid remeshing.
use crate::{Parcel, SpillSource, WaterError, WaterWorld};
use engine_core::Vec2;

impl WaterWorld {
    /// Atomically enable/disable the preallocated pools. Indices and geometry
    /// remain stable. Removed supports release each wet column into one free
    /// falling parcel; this is a transfer, NOT injection, drainage or cleanup.
    /// Returning `Capacity` or `InvalidInput` leaves ALL state unchanged. Callers
    /// must retain the old support geometry and retry after stepping on capacity
    /// pressure, not render a change whose physical update failed.
    ///
    /// Re-enabled supports start dry, with no old occupancy or surface velocity.
    /// Released water has its column's horizontal velocity and no imposed kick;
    /// it is not confined by an outlet channel. No allocations after construction.
    pub fn set_pool_supports(&mut self, enabled: &[bool]) -> Result<(), WaterError> {
        if enabled.len() != self.pools.len() {
            return Err(WaterError::InvalidInput);
        }
        let needed: usize = self
            .pools
            .iter()
            .zip(enabled)
            .filter(|(p, on)| p.enabled && !**on)
            .map(|(p, _)| p.volume.iter().filter(|v| **v > 0.0).count())
            .sum();
        if needed > self.config.max_parcels - self.parcels.len() {
            return Err(WaterError::Capacity);
        }
        // Detach old streams even if a support is re-enabled before next step.
        // They keep moving, but cannot attach to the reborn source's new flow.
        for spill in &mut self.spills {
            if let Some(s) = spill
                && let SpillSource::Outlet { pool, .. } = s.source
                && self.pools[pool].enabled != enabled[pool]
            {
                *spill = None;
            }
        }
        for (pool, &on) in self.pools.iter_mut().zip(enabled) {
            if pool.enabled == on {
                continue;
            }
            for c in pool.columns() {
                if c.volume > 0.0 {
                    self.parcels.push(Parcel {
                        position: Vec2::new(
                            (c.left + c.width * 0.5) as f32,
                            ((c.bed + c.surface) * 0.5) as f32,
                        ),
                        velocity: Vec2::new(c.velocity as f32, 0.0),
                        volume: c.volume,
                        duration: 1.0 / 60.0,
                        horizontal_bounds: None,
                    });
                    self.spills.push(None);
                }
            }
            pool.volume.fill(0.0);
            pool.velocity.fill(0.0);
            pool.flux.fill(0.0);
            pool.donor_scale.fill(0.0);
            pool.outlet_credit = [crate::drips::Credit::default(); 2];
            pool.displaced.fill(0.0);
            // Keep any lazily allocated occupancy scratch available for reuse.
            pool.displacement.clear();
            pool.enabled = on;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
