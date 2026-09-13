//! Deterministic, edge-connected components. Splitting transfers cells; it does
//! not destroy material or award any recovery.

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct DetachedTerrain {
    pub terrain: Terrain,
    /// Origin of the cropped field in the original field's local coordinates.
    pub parent_offset: Vec2,
}

struct Component {
    indices: Vec<u32>,
    bounds: CellBounds,
}

impl Terrain {
    /// Keep the largest edge-connected component in this field and transfer all
    /// others into tightly cropped fields. Corner contact alone does not connect
    /// cells. Ties and fragment order use each component's first row-major cell.
    ///
    /// The retained field's dimensions and coordinates stay unchanged. New fields
    /// start at revision zero and preserve every cell's material and durability.
    /// Only transferred cells dirty the retained field. An empty or connected
    /// field is a no-op. Call after actual removals, not after durability changes.
    pub fn detach_disconnected(&mut self) -> Result<Vec<DetachedTerrain>, TerrainError> {
        let mut visited = vec![false; self.cells.len()];
        let mut components: Vec<Component> = Vec::new();
        let mut largest = 0;
        for start in 0..self.cells.len() {
            if visited[start] || self.cells[start].material == MaterialId::VOID {
                continue;
            }
            let coord = CellCoord::new(
                (start as u32 % self.width) as i32,
                (start as u32 / self.width) as i32,
            );
            let mut component = Component {
                indices: vec![start as u32],
                bounds: CellBounds {
                    min: coord,
                    max: coord,
                },
            };
            visited[start] = true;
            let mut head = 0;
            while head < component.indices.len() {
                let index = component.indices[head];
                head += 1;
                let (x, y) = (index % self.width, index / self.width);
                component.bounds.min.x = component.bounds.min.x.min(x as i32);
                component.bounds.min.y = component.bounds.min.y.min(y as i32);
                component.bounds.max.x = component.bounds.max.x.max(x as i32);
                component.bounds.max.y = component.bounds.max.y.max(y as i32);
                let neighbors = [
                    (x > 0).then(|| index - 1),
                    (x + 1 < self.width).then_some(index + 1),
                    (y > 0).then(|| index - self.width),
                    (y + 1 < self.height).then_some(index + self.width),
                ];
                for neighbor in neighbors.into_iter().flatten() {
                    let n = neighbor as usize;
                    if !visited[n] && self.cells[n].material != MaterialId::VOID {
                        visited[n] = true;
                        component.indices.push(neighbor);
                    }
                }
            }
            if components.is_empty() || component.indices.len() > components[largest].indices.len()
            {
                largest = components.len();
            }
            components.push(component);
        }
        if components.len() <= 1 {
            return Ok(Vec::new());
        }
        if self.revision == u64::MAX {
            return Err(TerrainError("terrain revision exhausted"));
        }
        // Build and validate every destination before mutating the source.
        let mut fragments = Vec::with_capacity(components.len() - 1);
        for (index, component) in components.iter().enumerate() {
            if index == largest {
                continue;
            }
            let mut bounds = component.bounds;
            if self.distances.is_some() {
                // Retain a void sample halo: tight material cropping would lose
                // the original edge-crossing positions around the fragment.
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
            for &source in &component.indices {
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
            fragments.push(DetachedTerrain {
                terrain,
                parent_offset: self.cell_center(bounds.min)
                    + Vec2::new(width as f32 - 1.0, height as f32 - 1.0) * (self.cell_size * 0.5),
            });
        }
        self.revision += 1;
        let columns = self.width.div_ceil(CHUNK_SIZE);
        for (index, component) in components.iter().enumerate() {
            if index == largest {
                continue;
            }
            for &index in &component.indices {
                self.cells[index as usize] = Cell::VOID;
                if let Some(distances) = &mut self.distances {
                    distances[index as usize] = -distances[index as usize].abs();
                }
                let chunk =
                    (index / self.width / CHUNK_SIZE) * columns + (index % self.width / CHUNK_SIZE);
                self.chunk_revisions[chunk as usize] = self.revision;
            }
        }
        Ok(fragments)
    }
}

#[cfg(test)]
mod tests;
