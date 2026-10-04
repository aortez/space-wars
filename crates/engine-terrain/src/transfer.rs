//! Explicit material transfer. A caller chooses which cells to release; their
//! material, damage and spatial ownership survive the change of representation.

use super::*;

/// A cell whose material ownership has left a field. The destination chooses
/// its mechanics; cell_size² is the same unit-depth quantity as in the source.
/// This carries nominal cell quantity, not an interpolated surface's area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetachedCell {
    pub cell: Cell,
    pub cell_size: f32,
    /// Original cell center in the source body's local frame.
    pub parent_offset: Vec2,
}

/// One cell's quantity and state returning to a field. The caller owns the
/// departing representation and retires it only after this addition succeeds.
/// Particle models can accumulate whole cell quantities at this same boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellDeposit {
    pub coordinate: CellCoord,
    pub cell: Cell,
    pub cell_size: f32,
}

/// A damage edit's field changes and the material that left the field. The
/// samples retain their state immediately before the releasing hit: that hit
/// breaks their attachment, not their material identity or quantity. Surviving
/// cells retain ordinary partial damage. `edit.removed` counts transfer out of
/// the field; callers must not also credit those cells as destroyed/recovered.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleasedCells {
    pub edit: EditResult,
    pub cells: Vec<DetachedCell>,
}

impl Terrain {
    /// Add conserved material into vacant cells without changing its damage.
    /// Sizes must match exactly: resampling, mixing and material conversion are
    /// explicit caller policies. Duplicates, occupied/out-of-bounds destinations,
    /// unknown materials and invalid durability reject the entire batch.
    /// Refresh derived geometry after success. No revision changes on failure
    /// or an empty batch; no mining/removal quantity is produced.
    pub fn deposit_cells(&mut self, deposits: &[CellDeposit]) -> Result<(), TerrainError> {
        if deposits.is_empty() {
            return Ok(());
        }
        if self.revision == u64::MAX {
            return Err(TerrainError("terrain revision exhausted"));
        }
        let mut indices = std::collections::BTreeSet::new();
        for deposit in deposits {
            if deposit.cell_size != self.cell_size
                || self.cell(deposit.coordinate) != Some(Cell::VOID)
                || deposit.cell.durability == 0
                || self
                    .material(deposit.cell.material)
                    .is_none_or(|material| deposit.cell.durability > material.hardness)
            {
                return Err(TerrainError("invalid material deposit"));
            }
            let index =
                deposit.coordinate.y as usize * self.width as usize + deposit.coordinate.x as usize;
            if !indices.insert(index) {
                return Err(TerrainError("duplicate material deposit"));
            }
        }
        self.revision += 1;
        for deposit in deposits {
            let c = deposit.coordinate;
            let index = c.y as usize * self.width as usize + c.x as usize;
            self.cells[index] = deposit.cell;
            // A newly packed cell has a midpoint boundary sample, independent
            // of the old crater depth. Geometry dependencies include neighboring
            // chunks, so their contours refresh without changing their material.
            if let Some(distances) = &mut self.distances {
                distances[index] = self.cell_size * 0.5;
            }
            let chunk =
                c.y as u32 / CHUNK_SIZE * self.width.div_ceil(CHUNK_SIZE) + c.x as u32 / CHUNK_SIZE;
            self.chunk_revisions[chunk as usize] = self.revision;
        }
        Ok(())
    }

    /// Apply the ordinary damage/removal and surface-cut rules, retaining every
    /// departing cell as transferable material. Does not detach remaining
    /// islands; callers can do that once after a batch of edits.
    pub fn apply_releasing(&mut self, edit: TerrainEdit) -> Result<ReleasedCells, TerrainError> {
        let mut cells = Vec::new();
        let edit = self.apply_recording_release(edit, Some(&mut cells))?;
        Ok(ReleasedCells { edit, cells })
    }
    /// Transfer occupied cells into individual samples for grains, particles,
    /// or another representation. Preserves material, durability and location;
    /// it does not award mining yield. Order is row-major and duplicates/void
    /// cells are ignored. Invalid input leaves the field unchanged.
    ///
    /// Destinations are prepared before committing the source edit. A caller
    /// with a bounded destination should prepare on a cloned field, admit the
    /// whole transfer, then refresh geometry/colliders at its normal edit boundary.
    pub fn extract_individual_cells(
        &mut self,
        coordinates: &[CellCoord],
    ) -> Result<Vec<DetachedCell>, TerrainError> {
        let indices = self.transfer_indices(coordinates)?;
        let samples = indices
            .iter()
            .map(|&index| DetachedCell {
                cell: self.cells[index as usize],
                cell_size: self.cell_size,
                parent_offset: self.cell_center(CellCoord::new(
                    (index % self.width) as i32,
                    (index / self.width) as i32,
                )),
            })
            .collect();
        self.commit_transfer(&indices);
        Ok(samples)
    }

    /// Transfer selected occupied cells into independent connected fields.
    ///
    /// Coordinates must belong to this field. Duplicates and void cells are
    /// ignored. All destinations are prepared before the source changes, so an
    /// error leaves it intact. An empty selection does not advance its revision.
    /// No material is destroyed or credited as mining yield.
    ///
    /// The largest selected component is returned first; ties and the remaining
    /// order use each component's first row-major cell. New fields start at
    /// revision zero. Remaining source components are not separated here; the
    /// caller can run `detach_disconnected` once after a batch of transfers.
    pub fn extract_cells(
        &mut self,
        coordinates: &[CellCoord],
    ) -> Result<Vec<DetachedTerrain>, TerrainError> {
        let indices = self.transfer_indices(coordinates)?;
        let Some(&first) = indices.first() else {
            return Ok(Vec::new());
        };
        let coordinate =
            |index: u32| CellCoord::new((index % self.width) as i32, (index / self.width) as i32);
        let mut bounds = CellBounds {
            min: coordinate(first),
            max: coordinate(first),
        };
        for &index in &indices {
            let cell = coordinate(index);
            bounds.min.x = bounds.min.x.min(cell.x);
            bounds.min.y = bounds.min.y.min(cell.y);
            bounds.max.x = bounds.max.x.max(cell.x);
            bounds.max.y = bounds.max.y.max(cell.y);
        }
        let mut selected = self.copy_region_cells(&indices, bounds)?;
        let mut others = selected.terrain.detach_disconnected()?;
        for detached in &mut others {
            detached.parent_offset += selected.parent_offset;
        }
        // The temporary separation happened before any destination was published.
        selected.terrain.revision = 0;
        selected.terrain.chunk_revisions.fill(0);
        let mut result = vec![selected];
        result.extend(others);

        self.commit_transfer(&indices);
        Ok(result)
    }

    fn transfer_indices(&self, coordinates: &[CellCoord]) -> Result<Vec<u32>, TerrainError> {
        let mut indices = Vec::with_capacity(coordinates.len());
        for &coordinate in coordinates {
            let cell = self
                .cell(coordinate)
                .ok_or(TerrainError("transfer coordinate outside field"))?;
            if cell.material != MaterialId::VOID {
                indices.push(coordinate.y as u32 * self.width + coordinate.x as u32);
            }
        }
        indices.sort_unstable();
        indices.dedup();
        if !indices.is_empty() && self.revision == u64::MAX {
            return Err(TerrainError("terrain revision exhausted"));
        }
        Ok(indices)
    }

    fn commit_transfer(&mut self, indices: &[u32]) {
        if indices.is_empty() {
            return;
        }
        self.revision += 1;
        let columns = self.width.div_ceil(CHUNK_SIZE);
        for &index in indices {
            self.cells[index as usize] = Cell::VOID;
            if let Some(distances) = &mut self.distances {
                distances[index as usize] = -distances[index as usize].abs();
            }
            let chunk =
                (index / self.width / CHUNK_SIZE) * columns + index % self.width / CHUNK_SIZE;
            self.chunk_revisions[chunk as usize] = self.revision;
        }
    }

    pub(super) fn copy_region_cells(
        &self,
        indices: &[u32],
        mut bounds: CellBounds,
    ) -> Result<DetachedTerrain, TerrainError> {
        if self.distances.is_some() {
            // Retain the original edge crossings around each owned sample.
            bounds.min.x = (bounds.min.x - 1).max(0);
            bounds.min.y = (bounds.min.y - 1).max(0);
            bounds.max.x = (bounds.max.x + 1).min(self.width as i32 - 1);
            bounds.max.y = (bounds.max.y + 1).min(self.height as i32 - 1);
        }
        let width = (bounds.max.x - bounds.min.x + 1) as u32;
        let height = (bounds.max.y - bounds.min.y + 1) as u32;
        let mut terrain = Terrain::generate(
            width,
            height,
            self.cell_size,
            self.materials.clone(),
            |_| MaterialId::VOID,
        )?;
        for &source in indices {
            let x = source % self.width - bounds.min.x as u32;
            let y = source / self.width - bounds.min.y as u32;
            terrain.cells[(y * width + x) as usize] = self.cells[source as usize];
        }
        if self.distances.is_some() {
            terrain.version = 2;
            terrain.distances = Some(
                (0..height)
                    .flat_map(|y| (0..width).map(move |x| (x, y)))
                    .map(|(x, y)| {
                        let c = CellCoord::new(x as i32, y as i32);
                        let original = CellCoord::new(c.x + bounds.min.x, c.y + bounds.min.y);
                        let solid = terrain.cell(c).unwrap().material != MaterialId::VOID;
                        self.bound_distance(self.distance(original), solid)
                    })
                    .collect(),
            );
            terrain.validate()?;
        }
        Ok(DetachedTerrain {
            terrain,
            parent_offset: self.cell_center(bounds.min)
                + Vec2::new(width as f32 - 1.0, height as f32 - 1.0) * (self.cell_size * 0.5),
        })
    }
}

#[cfg(test)]
mod tests;
