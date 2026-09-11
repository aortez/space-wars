//! A cell-centred contour with explicit material ownership. Exposed convex
//! corners are cut at edge midpoints; concave corners receive two half-triangles.
//! This is the filled marching-squares contour with disconnected diagonal cases.
//! Cell centres and edge connectivity survive; no contour operation edits matter.

use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TerrainSurface {
    #[default]
    Blocks,
    Contour,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SolidPolygon {
    /// Counterclockwise vertices in the field's local world coordinates.
    pub vertices: Vec<Vec2>,
    pub material: MaterialId,
    /// Stable material owner, including for a triangle extending into a void cell.
    pub source: CellCoord,
    /// A clipped cell carries one full cell's mass. Concave fill carries none.
    pub owns_cell: bool,
}

const CORNERS: [(i32, i32); 4] = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
const SIDES: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

fn offset(c: CellCoord, x: i32, y: i32) -> CellCoord {
    CellCoord::new(c.x + x, c.y + y)
}

fn solid(t: &Terrain, c: CellCoord) -> bool {
    t.cell(c).is_some_and(|c| c.material != MaterialId::VOID)
}

fn clipped(t: &Terrain, c: CellCoord, x: i32, y: i32) -> bool {
    !solid(t, offset(c, x, 0)) && !solid(t, offset(c, 0, y))
}

fn has_clipped_corner(t: &Terrain, c: CellCoord) -> bool {
    CORNERS.iter().any(|&(x, y)| clipped(t, c, x, y))
}

fn cell_polygon(t: &Terrain, c: CellCoord) -> Vec<Vec2> {
    let center = t.cell_center(c);
    let half = t.cell_size() * 0.5;
    let corners = CORNERS.map(|(x, y)| Vec2::new(x as f32, y as f32));
    let mut vertices = Vec::with_capacity(8);
    for i in 0..4 {
        let (x, y) = CORNERS[i];
        if clipped(t, c, x, y) {
            for adjacent in [corners[(i + 3) % 4], corners[(i + 1) % 4]] {
                let p = center + (corners[i] + adjacent) * (half * 0.5);
                if vertices.last() != Some(&p) {
                    vertices.push(p);
                }
            }
        } else {
            vertices.push(center + corners[i] * half);
        }
    }
    if vertices.first() == vertices.last() {
        vertices.pop();
    }
    vertices
}

fn fills(t: &Terrain, owner: CellCoord) -> Vec<SolidPolygon> {
    let mut result = Vec::new();
    for (dx, dy) in SIDES {
        let void = offset(owner, -dx, -dy);
        // Never extend beyond the finite material field.
        if t.cell(void).is_none() || solid(t, void) {
            continue;
        }
        for sign in [-1, 1] {
            let (ox, oy) = (-dy * sign, dx * sign);
            if !solid(t, offset(void, ox, oy)) || !solid(t, offset(owner, ox, oy)) {
                continue;
            }
            let center = t.cell_center(void);
            let side = Vec2::new(dx as f32, dy as f32);
            let other = Vec2::new(ox as f32, oy as f32);
            let half = t.cell_size() * 0.5;
            let mut vertices = vec![
                center + (side + other) * half,
                center + side * half,
                center + (side + other) * (half * 0.5),
            ];
            if cross(vertices[1] - vertices[0], vertices[2] - vertices[0]) < 0.0 {
                vertices.swap(1, 2);
            }
            result.push(SolidPolygon {
                vertices,
                material: t.cell(owner).unwrap().material,
                source: owner,
                owns_cell: false,
            });
        }
    }
    result
}

pub(super) fn contour_chunk(t: &Terrain, id: ChunkId) -> (Vec<SolidRect>, Vec<SolidPolygon>) {
    let rectangles = rectangles_matching(t, id, |c| !has_clipped_corner(t, c));
    let mut polygons = Vec::new();
    let bounds = t.chunk_bounds(id).expect("valid chunk");
    for y in bounds.min.y..=bounds.max.y {
        for x in bounds.min.x..=bounds.max.x {
            let source = CellCoord::new(x, y);
            if !solid(t, source) {
                continue;
            }
            if has_clipped_corner(t, source) {
                polygons.push(SolidPolygon {
                    vertices: cell_polygon(t, source),
                    material: t.cell(source).unwrap().material,
                    source,
                    owns_cell: true,
                });
            }
            polygons.extend(fills(t, source));
        }
    }
    (rectangles, polygons)
}

pub(super) fn source_cell(t: &Terrain, surface: TerrainSurface, p: Vec2) -> Option<CellCoord> {
    let cell = t.local_to_cell(p)?;
    if surface == TerrainSurface::Blocks {
        return solid(t, cell).then_some(cell);
    }
    if solid(t, cell) {
        return contains(&cell_polygon(t, cell), p).then_some(cell);
    }
    // Only a cardinal neighbour can own concave fill in this void cell.
    for (dx, dy) in SIDES {
        let owner = offset(cell, dx, dy);
        if solid(t, owner) && fills(t, owner).iter().any(|f| contains(&f.vertices, p)) {
            return Some(owner);
        }
    }
    None
}

fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

fn contains(vertices: &[Vec2], p: Vec2) -> bool {
    vertices.iter().enumerate().all(|(i, &a)| {
        let edge = vertices[(i + 1) % vertices.len()] - a;
        cross(edge, p - a) >= -edge.length_squared() * 1.0e-6
    })
}

/// Re-anchor an attachment on a surviving cell after neighbours change its
/// contour. Only exposed edges qualify, not internal decomposition seams.
pub(super) fn project_source(
    t: &Terrain,
    owner: CellCoord,
    point: Vec2,
    normal: Vec2,
) -> Option<(Vec2, Vec2)> {
    if !solid(t, owner) {
        return None;
    }
    let mut polygons = vec![cell_polygon(t, owner)];
    polygons.extend(fills(t, owner).into_iter().map(|p| p.vertices));
    let mut best: Option<(f32, Vec2, Vec2)> = None;
    for vertices in polygons {
        for (i, &a) in vertices.iter().enumerate() {
            let edge = vertices[(i + 1) % vertices.len()] - a;
            let outward = Vec2::new(edge.y, -edge.x).normalized();
            if outward.dot(normal) < 0.25 {
                continue;
            }
            // A square side may be half covered by a neighbour's concave fill.
            for half in 0..2 {
                let start = a + edge * (half as f32 * 0.5);
                let delta = edge * 0.5;
                let middle = start + delta * 0.5;
                if source_cell(
                    t,
                    TerrainSurface::Contour,
                    middle + outward * (t.cell_size() * 0.001),
                )
                .is_some()
                {
                    continue;
                }
                let candidate = start
                    + delta * ((point - start).dot(delta) / delta.length_squared()).clamp(0.0, 1.0);
                let distance = candidate.distance_to(point);
                if distance <= t.cell_size() * 0.75 && best.is_none_or(|v| distance < v.0) {
                    best = Some((distance, candidate, outward));
                }
            }
        }
    }
    best.map(|(_, p, n)| (p, n))
}

#[cfg(test)]
mod tests;
