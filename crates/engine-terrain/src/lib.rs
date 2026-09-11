//! Deterministic material fields. Coordinates and edits are independent of gravity,
//! rendering, rigid bodies, and scenario economics.

use engine_core::Vec2;
use serde::{Deserialize, Deserializer, Serialize};

mod connectivity;
pub use connectivity::DetachedTerrain;

pub const CHUNK_SIZE: u32 = 32;
const FORMAT_VERSION: u32 = 1;
const MAX_SIDE: u32 = 4096;
const MAX_BRUSH_COORDINATE: i32 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MaterialId(pub u8);

impl MaterialId {
    pub const VOID: Self = Self(0);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Material {
    pub id: MaterialId,
    /// Initial integer work needed to remove one cell.
    pub hardness: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub material: MaterialId,
    pub durability: u8,
}

impl Cell {
    pub const VOID: Self = Self {
        material: MaterialId::VOID,
        durability: 0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
}

impl CellCoord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChunkId(pub u32);

/// Inclusive cell bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellBounds {
    pub min: CellCoord,
    pub max: CellCoord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Brush {
    Circle {
        center: CellCoord,
        radius: u32,
    },
    Capsule {
        start: CellCoord,
        end: CellCoord,
        radius: u32,
    },
}

impl Brush {
    fn ends(self) -> (CellCoord, CellCoord, u32) {
        match self {
            Self::Circle { center, radius } => (center, center, radius),
            Self::Capsule { start, end, radius } => (start, end, radius),
        }
    }

    fn bounds(self) -> Result<CellBounds, TerrainError> {
        let (a, b, radius) = self.ends();
        if [a.x, a.y, b.x, b.y]
            .into_iter()
            .any(|v| v.unsigned_abs() > MAX_BRUSH_COORDINATE as u32)
            || radius > MAX_BRUSH_COORDINATE as u32
        {
            return Err(TerrainError("brush exceeds coordinate limits"));
        }
        let radius = radius as i32;
        Ok(CellBounds {
            min: CellCoord::new(a.x.min(b.x) - radius, a.y.min(b.y) - radius),
            max: CellCoord::new(a.x.max(b.x) + radius, a.y.max(b.y) + radius),
        })
    }

    fn contains(self, point: CellCoord) -> bool {
        let (a, b, radius) = self.ends();
        let dx = i128::from(b.x) - i128::from(a.x);
        let dy = i128::from(b.y) - i128::from(a.y);
        let px = i128::from(point.x) - i128::from(a.x);
        let py = i128::from(point.y) - i128::from(a.y);
        let length_squared = dx * dx + dy * dy;
        let projection = px * dx + py * dy;
        let radius_squared = i128::from(radius).pow(2);
        if projection <= 0 || length_squared == 0 {
            px * px + py * py <= radius_squared
        } else if projection >= length_squared {
            (px - dx).pow(2) + (py - dy).pow(2) <= radius_squared
        } else {
            (px * dy - py * dx).pow(2) <= radius_squared * length_squared
        }
    }

    fn coordinates(
        self,
        width: u32,
        height: u32,
    ) -> Result<impl Iterator<Item = CellCoord>, TerrainError> {
        let bounds = self.bounds()?;
        let xs = bounds.min.x.max(0)..=bounds.max.x.min(width as i32 - 1);
        let ys = bounds.min.y.max(0)..=bounds.max.y.min(height as i32 - 1);
        Ok(ys
            .flat_map(move |y| xs.clone().map(move |x| CellCoord::new(x, y)))
            .filter(move |&coord| self.contains(coord)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditMode {
    Remove,
    Damage(u8),
}

/// Already quantized in the field's coordinates, including when queued by an impact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerrainEdit {
    pub brush: Brush,
    pub mode: EditMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedMaterial {
    pub material: MaterialId,
    pub cells: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditResult {
    pub revision: u64,
    pub changed_cells: u32,
    pub bounds: Option<CellBounds>,
    pub dirty_chunks: Vec<ChunkId>,
    /// Physical quantities only. The caller determines recovery and storage policy.
    pub removed: Vec<RemovedMaterial>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainError(pub &'static str);

impl std::fmt::Display for TerrainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for TerrainError {}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Terrain {
    version: u32,
    width: u32,
    height: u32,
    cell_size: f32,
    materials: Vec<Material>,
    cells: Vec<Cell>,
    revision: u64,
    chunk_revisions: Vec<u64>,
}

#[derive(Deserialize)]
struct TerrainState {
    version: u32,
    width: u32,
    height: u32,
    cell_size: f32,
    materials: Vec<Material>,
    cells: Vec<Cell>,
    revision: u64,
    chunk_revisions: Vec<u64>,
}

impl<'de> Deserialize<'de> for Terrain {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let state = TerrainState::deserialize(deserializer)?;
        let terrain = Self {
            version: state.version,
            width: state.width,
            height: state.height,
            cell_size: state.cell_size,
            materials: state.materials,
            cells: state.cells,
            revision: state.revision,
            chunk_revisions: state.chunk_revisions,
        };
        terrain.validate().map_err(serde::de::Error::custom)?;
        Ok(terrain)
    }
}

impl Terrain {
    pub fn generate(
        width: u32,
        height: u32,
        cell_size: f32,
        mut materials: Vec<Material>,
        mut generator: impl FnMut(CellCoord) -> MaterialId,
    ) -> Result<Self, TerrainError> {
        validate_dimensions(width, height, cell_size)?;
        materials.sort_by_key(|material| material.id);
        validate_materials(&materials)?;
        let mut cells = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let id = generator(CellCoord::new(x as i32, y as i32));
                cells.push(if id == MaterialId::VOID {
                    Cell::VOID
                } else {
                    let material = materials
                        .iter()
                        .find(|material| material.id == id)
                        .ok_or(TerrainError("generator returned an undefined material"))?;
                    Cell {
                        material: id,
                        durability: material.hardness,
                    }
                });
            }
        }
        Ok(Self {
            version: FORMAT_VERSION,
            width,
            height,
            cell_size,
            materials,
            cells,
            revision: 0,
            chunk_revisions: vec![
                0;
                (width.div_ceil(CHUNK_SIZE) * height.div_ceil(CHUNK_SIZE))
                    as usize
            ],
        })
    }

    fn validate(&self) -> Result<(), TerrainError> {
        validate_dimensions(self.width, self.height, self.cell_size)?;
        validate_materials(&self.materials)?;
        if self.version != FORMAT_VERSION
            || self.cells.len() != (self.width * self.height) as usize
            || self.chunk_revisions.len()
                != (self.width.div_ceil(CHUNK_SIZE) * self.height.div_ceil(CHUNK_SIZE)) as usize
            || self
                .chunk_revisions
                .iter()
                .any(|revision| *revision > self.revision)
        {
            return Err(TerrainError("invalid terrain state layout or revision"));
        }
        for cell in &self.cells {
            if cell.material == MaterialId::VOID {
                if *cell != Cell::VOID {
                    return Err(TerrainError("void has durability"));
                }
            } else if !self.materials.iter().any(|material| {
                material.id == cell.material
                    && cell.durability > 0
                    && cell.durability <= material.hardness
            }) {
                return Err(TerrainError("invalid cell material or durability"));
            }
        }
        Ok(())
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }
    pub fn material(&self, id: MaterialId) -> Option<Material> {
        self.materials
            .iter()
            .find(|material| material.id == id)
            .copied()
    }
    pub fn chunk_count(&self) -> usize {
        self.chunk_revisions.len()
    }
    pub fn cell_bytes(&self) -> usize {
        self.cells.len() * std::mem::size_of::<Cell>()
    }
    pub fn chunk_revision(&self, id: ChunkId) -> Option<u64> {
        self.chunk_revisions.get(id.0 as usize).copied()
    }

    pub fn cell(&self, coord: CellCoord) -> Option<Cell> {
        if coord.x < 0
            || coord.y < 0
            || coord.x >= self.width as i32
            || coord.y >= self.height as i32
        {
            None
        } else {
            Some(self.cells[(coord.y as u32 * self.width + coord.x as u32) as usize])
        }
    }

    /// Cell centers, with the whole rectangular field centered on local zero.
    pub fn cell_center(&self, coord: CellCoord) -> Vec2 {
        Vec2::new(
            coord.x as f32 + 0.5 - self.width as f32 * 0.5,
            coord.y as f32 + 0.5 - self.height as f32 * 0.5,
        ) * self.cell_size
    }

    /// Quantize once before queueing an edit. Points outside the field are useful
    /// for brushes crossing its edge; absurd or nonfinite coordinates are rejected.
    pub fn local_to_cell(&self, point: Vec2) -> Option<CellCoord> {
        let x = (point.x / self.cell_size + self.width as f32 * 0.5).floor();
        let y = (point.y / self.cell_size + self.height as f32 * 0.5).floor();
        (x.is_finite()
            && y.is_finite()
            && x.abs() <= MAX_BRUSH_COORDINATE as f32
            && y.abs() <= MAX_BRUSH_COORDINATE as f32)
            .then_some(CellCoord::new(x as i32, y as i32))
    }

    pub fn chunk_bounds(&self, id: ChunkId) -> Option<CellBounds> {
        self.chunk_revision(id)?;
        let columns = self.width.div_ceil(CHUNK_SIZE);
        let x = id.0 % columns * CHUNK_SIZE;
        let y = id.0 / columns * CHUNK_SIZE;
        Some(CellBounds {
            min: CellCoord::new(x as i32, y as i32),
            max: CellCoord::new(
                (x + CHUNK_SIZE).min(self.width) as i32 - 1,
                (y + CHUNK_SIZE).min(self.height) as i32 - 1,
            ),
        })
    }

    /// Read the occupied cells an edit would affect, in the same row-major order
    /// and with the same bounds and inclusion rules as `apply`.
    pub fn brush_cells(
        &self,
        brush: Brush,
    ) -> Result<impl Iterator<Item = (CellCoord, Cell)> + '_, TerrainError> {
        Ok(brush
            .coordinates(self.width, self.height)?
            .filter_map(|coord| {
                let cell = self.cell(coord)?;
                (cell.material != MaterialId::VOID).then_some((coord, cell))
            }))
    }

    pub fn apply(&mut self, edit: TerrainEdit) -> Result<EditResult, TerrainError> {
        let coordinates = edit.brush.coordinates(self.width, self.height)?;
        let mut result = EditResult {
            revision: self.revision,
            changed_cells: 0,
            bounds: None,
            dirty_chunks: Vec::new(),
            removed: Vec::new(),
        };
        if edit.mode == EditMode::Damage(0) {
            return Ok(result);
        }
        if self.revision == u64::MAX {
            return Err(TerrainError("terrain revision exhausted"));
        }
        let mut dirty = vec![false; self.chunk_count()];
        let mut removed = [0u32; 256];
        for coord in coordinates {
            let (x, y) = (coord.x, coord.y);
            let index = y as usize * self.width as usize + x as usize;
            let cell = self.cells[index];
            if cell.material == MaterialId::VOID {
                continue;
            }
            let durability = match edit.mode {
                EditMode::Remove => 0,
                EditMode::Damage(work) => cell.durability.saturating_sub(work),
            };
            self.cells[index] = if durability == 0 {
                removed[cell.material.0 as usize] += 1;
                Cell::VOID
            } else {
                Cell { durability, ..cell }
            };
            result.changed_cells += 1;
            let changed = result.bounds.get_or_insert(CellBounds {
                min: coord,
                max: coord,
            });
            changed.min.x = changed.min.x.min(x);
            changed.min.y = changed.min.y.min(y);
            changed.max.x = changed.max.x.max(x);
            changed.max.y = changed.max.y.max(y);
            let chunk =
                y as u32 / CHUNK_SIZE * self.width.div_ceil(CHUNK_SIZE) + x as u32 / CHUNK_SIZE;
            dirty[chunk as usize] = true;
        }
        if result.changed_cells > 0 {
            self.revision += 1;
            result.revision = self.revision;
            for (index, changed) in dirty.into_iter().enumerate() {
                if changed {
                    self.chunk_revisions[index] = self.revision;
                    result.dirty_chunks.push(ChunkId(index as u32));
                }
            }
            for (id, cells) in removed.into_iter().enumerate() {
                if cells > 0 {
                    result.removed.push(RemovedMaterial {
                        material: MaterialId(id as u8),
                        cells,
                    });
                }
            }
        }
        Ok(result)
    }

    /// Versioned FNV-1a over canonical little-endian configuration and cell data.
    /// Deliberately excludes revisions and derived caches: this identifies content.
    pub fn hash(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        let mut write = |bytes: &[u8]| {
            for byte in bytes {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
            }
        };
        for value in [
            self.version,
            self.width,
            self.height,
            self.cell_size.to_bits(),
            self.materials.len() as u32,
        ] {
            write(&value.to_le_bytes());
        }
        for material in &self.materials {
            write(&[material.id.0, material.hardness]);
        }
        for cell in &self.cells {
            write(&[cell.material.0, cell.durability]);
        }
        hash
    }
}

fn validate_dimensions(width: u32, height: u32, cell_size: f32) -> Result<(), TerrainError> {
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || !cell_size.is_finite()
        || !(0.0001..=1_000_000.0).contains(&cell_size)
    {
        Err(TerrainError("invalid field dimensions or cell size"))
    } else {
        Ok(())
    }
}

fn validate_materials(materials: &[Material]) -> Result<(), TerrainError> {
    if materials.len() > 255
        || materials
            .iter()
            .any(|m| m.id == MaterialId::VOID || m.hardness == 0)
        || materials.windows(2).any(|pair| pair[0].id >= pair[1].id)
    {
        Err(TerrainError("invalid or duplicate material definition"))
    } else {
        Ok(())
    }
}

/// Material-homogeneous solid rectangle, expressed as cell indices and extents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolidRect {
    pub min: CellCoord,
    pub width: u32,
    pub height: u32,
    pub material: MaterialId,
}

impl SolidRect {
    pub fn local_center(self, terrain: &Terrain) -> Vec2 {
        terrain.cell_center(self.min)
            + Vec2::new(self.width as f32 - 1.0, self.height as f32 - 1.0)
                * (terrain.cell_size() * 0.5)
    }
    pub fn half_extents(self, terrain: &Terrain) -> Vec2 {
        Vec2::new(self.width as f32, self.height as f32) * (terrain.cell_size() * 0.5)
    }
}

/// Bounded greedy rectangle cover. Every solid cell appears exactly once; holes
/// and chunk edges are represented exactly. Durability does not split geometry.
pub fn chunk_rectangles(terrain: &Terrain, id: ChunkId) -> Vec<SolidRect> {
    let Some(bounds) = terrain.chunk_bounds(id) else {
        return Vec::new();
    };
    let mut used = [false; (CHUNK_SIZE * CHUNK_SIZE) as usize];
    let index = |x: i32, y: i32| {
        ((y - bounds.min.y) as u32 * CHUNK_SIZE + (x - bounds.min.x) as u32) as usize
    };
    let mut rectangles = Vec::new();
    for y in bounds.min.y..=bounds.max.y {
        for x in bounds.min.x..=bounds.max.x {
            let material = terrain.cell(CellCoord::new(x, y)).unwrap().material;
            if used[index(x, y)] || material == MaterialId::VOID {
                continue;
            }
            let available = |cx, cy, used: &[bool]| {
                !used[index(cx, cy)]
                    && terrain.cell(CellCoord::new(cx, cy)).unwrap().material == material
            };
            let mut end_x = x;
            while end_x < bounds.max.x && available(end_x + 1, y, &used) {
                end_x += 1;
            }
            let mut end_y = y;
            while end_y < bounds.max.y && (x..=end_x).all(|cx| available(cx, end_y + 1, &used)) {
                end_y += 1;
            }
            for cy in y..=end_y {
                for cx in x..=end_x {
                    used[index(cx, cy)] = true;
                }
            }
            rectangles.push(SolidRect {
                min: CellCoord::new(x, y),
                width: (end_x - x + 1) as u32,
                height: (end_y - y + 1) as u32,
                material,
            });
        }
    }
    rectangles
}

#[derive(Debug, Clone)]
pub struct ChunkGeometry {
    pub id: ChunkId,
    pub revision: u64,
    pub rectangles: Vec<SolidRect>,
}

/// One cache per terrain instance. Recreate this cache when replacing/restoring
/// a field; revisions describe edits within that instance, not global identity.
#[derive(Debug, Clone)]
pub struct TerrainGeometry {
    chunks: Vec<ChunkGeometry>,
}

impl TerrainGeometry {
    pub fn new(terrain: &Terrain) -> Self {
        Self {
            chunks: (0..terrain.chunk_count())
                .map(|index| {
                    let id = ChunkId(index as u32);
                    ChunkGeometry {
                        id,
                        revision: terrain.chunk_revision(id).unwrap(),
                        rectangles: chunk_rectangles(terrain, id),
                    }
                })
                .collect(),
        }
    }

    pub fn chunks(&self) -> &[ChunkGeometry] {
        &self.chunks
    }
    pub fn rectangle_count(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.rectangles.len()).sum()
    }

    pub fn refresh(&mut self, terrain: &Terrain) -> Vec<ChunkId> {
        assert_eq!(
            self.chunks.len(),
            terrain.chunk_count(),
            "cache belongs to a different terrain"
        );
        let mut changed = Vec::new();
        for chunk in &mut self.chunks {
            let revision = terrain.chunk_revision(chunk.id).unwrap();
            if chunk.revision != revision {
                chunk.rectangles = chunk_rectangles(terrain, chunk.id);
                chunk.revision = revision;
                changed.push(chunk.id);
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests;
