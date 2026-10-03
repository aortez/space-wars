//! Explicit material transfer. A caller chooses which cells to release; their
//! material, damage and spatial ownership survive the change of representation.

use super::*;

impl Terrain {
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
        let Some(&first) = indices.first() else {
            return Ok(Vec::new());
        };
        if self.revision == u64::MAX {
            return Err(TerrainError("terrain revision exhausted"));
        }
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

        self.revision += 1;
        let columns = self.width.div_ceil(CHUNK_SIZE);
        for index in indices {
            self.cells[index as usize] = Cell::VOID;
            if let Some(distances) = &mut self.distances {
                distances[index as usize] = -distances[index as usize].abs();
            }
            let chunk =
                (index / self.width / CHUNK_SIZE) * columns + index % self.width / CHUNK_SIZE;
            self.chunk_revisions[chunk as usize] = self.revision;
        }
        Ok(result)
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
