//! Opt-in, local surface response to descending parcel collection. No new
//! liquid, splash particles, renderer effects or per-step allocations.
use crate::{Parcel, Pool, WaterConfig, WaterWorld};

impl WaterWorld {
    pub(crate) fn deposit(&mut self, pool: usize, column: usize, parcel: Parcel) {
        let pool = &mut self.pools[pool];
        self.impact_transfers += u64::from(pool.impact(column, parcel, self.config));
        pool.volume[column] += parcel.volume;
    }
}

impl Pool {
    fn impact(&mut self, column: usize, parcel: Parcel, config: WaterConfig) -> bool {
        if config.impact_response == 0.0 || parcel.velocity.y >= 0.0 || self.volume[column] == 0.0 {
            return false;
        }
        // Surface velocities live on faces, not columns. Only wet interior
        // faces participate; closed walls, spill laws, dry/raised barriers and
        // unrelated pools must not acquire a synthetic current.
        let neighbors = [
            column.checked_sub(1).map(|j| (column, j, -1.0)),
            (column + 1 < self.volume.len()).then_some((column + 1, column + 1, 1.0)),
        ];
        let mut changed = false;
        for (face, neighbor, direction) in neighbors.into_iter().flatten() {
            let wet_depth = self.surface(column).min(self.surface(neighbor)) - self.face_bed(face);
            if self.volume[neighbor] == 0.0 || wet_depth <= 0.0 {
                continue;
            }
            let receiving = (self.volume[column] + self.volume[neighbor]) * 0.5;
            let share = parcel.volume / (receiving + parcel.volume);
            // Shallow films should ripple, not acquire a near-free-fall jet
            // from a large incoming drop. Bound the perturbation by a local
            // gravity/depth speed scale as well as the ordinary solver cap.
            let kick = config.impact_response
                * (f64::from(-parcel.velocity.y) * share).min((config.gravity * wet_depth).sqrt());
            let old = self.velocity[face];
            self.velocity[face] =
                (old + direction * kick).clamp(-config.max_speed, config.max_speed);
            changed |= old != self.velocity[face];
        }
        // Downward momentum is redirected heuristically into two outward
        // kicks. Unequal depths/walls need not balance horizontal momentum.
        // The normal substep speed/donor limits still bound transported water.
        // Birth-step collections are solved on the next step, not twice here.
        changed
    }
}

#[cfg(test)]
mod tests;
