//! A bounded signed-distance band used only to locate the surface between samples.
//! Material identity, durability, connectivity and mass remain cell based.
use super::*;

impl Terrain {
    /// Attach shape samples when constructing a field, before creating its caches.
    /// Positive/zero input must agree with solid cells; negative input with void.
    pub fn with_surface_distances(
        mut self,
        mut distance: impl FnMut(Vec2) -> f32,
    ) -> Result<Self, TerrainError> {
        let mut samples = Vec::with_capacity(self.cells.len());
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let c = CellCoord::new(x, y);
                let value = distance(self.cell_center(c));
                let solid = self.cell(c).unwrap().material != MaterialId::VOID;
                if !value.is_finite() || (value >= 0.0) != solid {
                    return Err(TerrainError("surface samples disagree with material"));
                }
                samples.push(self.bound_distance(value, solid));
            }
        }
        self.version = 2;
        self.distances = Some(samples);
        Ok(self)
    }

    pub fn surface_sample_bytes(&self) -> usize {
        self.distances
            .as_ref()
            .map_or(0, |v| v.len() * std::mem::size_of::<f32>())
    }

    pub(super) fn bound_distance(&self, value: f32, solid: bool) -> f32 {
        // A tiny positive patch must survive for every owned cell. This also
        // avoids degenerate convex colliders at exact zero crossings.
        value.abs().clamp(self.cell_size * 0.001, self.cell_size) * if solid { 1.0 } else { -1.0 }
    }

    pub(super) fn distance(&self, c: CellCoord) -> f32 {
        let Some(cell) = self.cell(c) else {
            return -self.cell_size * 0.5;
        };
        self.distances.as_ref().map_or_else(
            || if cell.material == MaterialId::VOID { -0.5 } else { 0.5 } * self.cell_size,
            |v| v[(c.y as u32 * self.width + c.x as u32) as usize],
        )
    }

    pub(super) fn validate_distances(&self) -> Result<(), TerrainError> {
        if let Some(values) = &self.distances
            && (values.len() != self.cells.len()
                || values.iter().zip(&self.cells).any(|(&v, c)| {
                    !v.is_finite()
                        || v == 0.0
                        || v.abs() > self.cell_size
                        || (v > 0.0) != (c.material != MaterialId::VOID)
                }))
        {
            return Err(TerrainError("invalid surface distance samples"));
        }
        Ok(())
    }

    pub(super) fn cut_boundary(&mut self, brush: Brush, removed: &[CellCoord], dirty: &mut [bool]) {
        if removed.is_empty() {
            return;
        }
        let mut values = self.distances.take().expect("removed sample coordinates");
        let mut affected = std::collections::BTreeSet::new();
        for c in removed {
            for y in (c.y - 1).max(0)..=(c.y + 1).min(self.height as i32 - 1) {
                for x in (c.x - 1).max(0)..=(c.x + 1).min(self.width as i32 - 1) {
                    affected.insert((y * self.width as i32 + x) as usize);
                }
            }
        }
        let (mut a, mut b, radius) = brush.ends();
        // Canonical ordering makes reversed capsule strokes bit-identical.
        if (a.y, a.x) > (b.y, b.x) {
            std::mem::swap(&mut a, &mut b);
        }
        let start = self.cell_center(a);
        let delta = self.cell_center(b) - start;
        let radius = (radius as f32).max(0.5) * self.cell_size;
        for i in affected {
            let c = CellCoord::new(i as i32 % self.width as i32, i as i32 / self.width as i32);
            let point = self.cell_center(c);
            let fraction = if delta.length_squared() == 0.0 {
                0.0
            } else {
                ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0)
            };
            let cut = point.distance_to(start + delta * fraction) - radius;
            let solid = self.cells[i].material != MaterialId::VOID;
            let value = if solid {
                // Harder material inside a damage brush stays intact until removed.
                if cut > 0.0 {
                    values[i].min(cut)
                } else {
                    values[i]
                }
            } else if values[i] > 0.0 {
                cut.min(-self.cell_size * 0.001)
            } else {
                values[i].min(cut)
            };
            let value = self.bound_distance(value, solid);
            if value != values[i] {
                values[i] = value;
                let chunk = c.y as u32 / CHUNK_SIZE * self.width.div_ceil(CHUNK_SIZE)
                    + c.x as u32 / CHUNK_SIZE;
                dirty[chunk as usize] = true;
            }
        }
        self.distances = Some(values);
    }
}
