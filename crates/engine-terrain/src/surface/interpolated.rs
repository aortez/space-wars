//! Filled marching squares with variable edge crossings. Diagonal-only material
//! stays disconnected. Convex regions are partitioned by nearest solid sample,
//! retaining a real source cell for drawing, contacts, mining and attachments.
use super::*;

struct Region {
    vertices: Vec<Vec2>,
    owners: Vec<CellCoord>,
}

fn crossing(t: &Terrain, mut a: CellCoord, mut b: CellCoord) -> Vec2 {
    if (a.y, a.x) > (b.y, b.x) {
        std::mem::swap(&mut a, &mut b);
    }
    let da = t.distance(a);
    let fraction = da / (da - t.distance(b));
    t.cell_center(a) + (t.cell_center(b) - t.cell_center(a)) * fraction
}

fn regions(t: &Terrain, base: CellCoord) -> Vec<Region> {
    let cells = [
        base,
        offset(base, 1, 0),
        offset(base, 1, 1),
        offset(base, 0, 1),
    ];
    let mask = cells
        .iter()
        .enumerate()
        .fold(0, |m, (i, &c)| m | (u8::from(solid(t, c)) << i));
    if mask == 0 {
        return Vec::new();
    }
    if mask == 5 || mask == 10 {
        return (0..4)
            .filter(|&i| mask & (1 << i) != 0)
            .map(|i| Region {
                vertices: vec![
                    t.cell_center(cells[i]),
                    crossing(t, cells[i], cells[(i + 1) % 4]),
                    crossing(t, cells[(i + 3) % 4], cells[i]),
                ],
                owners: vec![cells[i]],
            })
            .collect();
    }
    let mut vertices = Vec::with_capacity(6);
    let mut owners = Vec::with_capacity(4);
    for i in 0..4 {
        let next = (i + 1) % 4;
        let inside = mask & (1 << i) != 0;
        if inside {
            vertices.push(t.cell_center(cells[i]));
            owners.push(cells[i]);
        }
        if inside != (mask & (1 << next) != 0) {
            vertices.push(crossing(t, cells[i], cells[next]));
        }
    }
    vec![Region { vertices, owners }]
}

fn nearest_half(vertices: Vec<Vec2>, owner: Vec2, other: Vec2) -> Vec<Vec2> {
    let middle = (owner + other) * 0.5;
    let normal = other - owner;
    let mut result = Vec::with_capacity(vertices.len() + 1);
    for (i, &a) in vertices.iter().enumerate() {
        let b = vertices[(i + 1) % vertices.len()];
        let da = (a - middle).dot(normal);
        let db = (b - middle).dot(normal);
        if da <= 0.0 {
            result.push(a);
        }
        if (da <= 0.0) != (db <= 0.0) {
            result.push(a + (b - a) * (da / (da - db)));
        }
    }
    result.dedup_by(|a, b| a == b);
    if result.len() > 1 && result.first() == result.last() {
        result.pop();
    }
    result
}

pub(crate) fn owned_polygons(t: &Terrain, owner: CellCoord) -> Vec<SolidPolygon> {
    if !solid(t, owner) {
        return Vec::new();
    }
    let mut polygons = Vec::new();
    for (dx, dy) in [(-1, -1), (0, -1), (0, 0), (-1, 0)] {
        for region in regions(t, offset(owner, dx, dy)) {
            if !region.owners.contains(&owner) {
                continue;
            }
            let mut vertices = region.vertices;
            for other in region.owners {
                if other != owner {
                    vertices = nearest_half(vertices, t.cell_center(owner), t.cell_center(other));
                }
            }
            polygons.push(SolidPolygon {
                vertices,
                source: owner,
                material: t.cell(owner).unwrap().material,
                owns_cell: polygons.is_empty(),
            });
        }
    }
    polygons
}

fn interior(t: &Terrain, c: CellCoord) -> bool {
    (-1..=1).all(|y| (-1..=1).all(|x| solid(t, offset(c, x, y))))
}

pub(crate) fn chunk(t: &Terrain, id: ChunkId) -> (Vec<SolidRect>, Vec<SolidPolygon>) {
    let rectangles = rectangles_matching(t, id, |c| interior(t, c));
    let bounds = t.chunk_bounds(id).expect("valid chunk");
    let mut polygons = Vec::new();
    for y in bounds.min.y..=bounds.max.y {
        for x in bounds.min.x..=bounds.max.x {
            let owner = CellCoord::new(x, y);
            if solid(t, owner) && !interior(t, owner) {
                polygons.extend(owned_polygons(t, owner));
            }
        }
    }
    (rectangles, polygons)
}

pub(crate) fn source_cell(t: &Terrain, p: Vec2) -> Option<CellCoord> {
    let base = t.local_to_cell(p - Vec2::new(0.5, 0.5) * t.cell_size())?;
    for region in regions(t, base) {
        if contains(&region.vertices, p) {
            return region.owners.into_iter().min_by(|&a, &b| {
                (p - t.cell_center(a))
                    .length_squared()
                    .total_cmp(&(p - t.cell_center(b)).length_squared())
            });
        }
    }
    None
}

#[cfg(test)]
mod tests;
