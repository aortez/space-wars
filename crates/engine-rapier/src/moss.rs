//! Terrain surface eligibility for moss tissue; no duplicate physiology.
use crate::root_uptake::ContactReason;
use engine_terrain::{CellCoord, Terrain, TerrainGeometry};

const NEIGHBORS: [[i32; 2]; 8] = [
    [-1, -1],
    [0, -1],
    [1, -1],
    [-1, 0],
    [1, 0],
    [-1, 1],
    [0, 1],
    [1, 1],
];

/// A moss cell is a lightweight point at its local cell center. It lies on a
/// surface when that center is outside canonical solid geometry and any of the
/// eight neighboring cell centers is inside it, of any material. Corner contact
/// lets a patch wrap convex edges; nothing bridges a wider air gap. One check per
/// tissue, however many neighbors touch, so contact never multiplies supply.
pub fn contact(terrain: &Terrain, geometry: &TerrainGeometry, cell: [i32; 2]) -> ContactReason {
    let solid = |c: [i32; 2]| {
        geometry
            .source_cell(terrain, terrain.cell_center(CellCoord::new(c[0], c[1])))
            .is_some()
    };
    if solid(cell) {
        return ContactReason::Enclosed;
    }
    let touching = NEIGHBORS.iter().any(|d| {
        let (Some(x), Some(y)) = (cell[0].checked_add(d[0]), cell[1].checked_add(d[1])) else {
            return false;
        };
        solid([x, y])
    });
    if touching {
        ContactReason::Eligible
    } else {
        ContactReason::Air
    }
}

/// Moss placement: inside the finite terrain domain and in surface contact.
pub fn site(terrain: &Terrain, geometry: &TerrainGeometry, cell: [i32; 2]) -> bool {
    terrain.cell(CellCoord::new(cell[0], cell[1])).is_some()
        && contact(terrain, geometry, cell) == ContactReason::Eligible
}

#[cfg(test)]
mod moss_surface_contract;
