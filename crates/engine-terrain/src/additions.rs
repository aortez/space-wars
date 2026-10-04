//! Convex patches added by a local material edit. Used to validate growth
//! without treating unchanged supporting terrain as a new obstruction.
use super::*;
use std::collections::BTreeSet;

type Point = [f64; 2];
type Polygon = Vec<Point>;

impl TerrainSurface {
    /// Preview the new solid area minus the previous surface as convex patches
    /// in field-local coordinates. Reads only local reconstruction samples; a
    /// rejected placement does not need a complete chunk-cache rebuild.
    /// Both fields must have the same layout. `changed` lists every edited
    /// material/sample coordinate. This is geometry, never material quantity.
    pub fn added_surface(
        self,
        previous: &Terrain,
        terrain: &Terrain,
        changed: &[CellCoord],
    ) -> Vec<Vec<Vec2>> {
        assert_eq!(
            (previous.width(), previous.height(), previous.cell_size()),
            (terrain.width(), terrain.height(), terrain.cell_size())
        );
        let halo = i32::from(self != TerrainSurface::Blocks);
        let mut tiles = BTreeSet::new();
        for c in changed {
            for y in c.y - halo..=c.y + halo {
                for x in c.x - halo..=c.x + halo {
                    // Interpolated samples can extend slightly beyond the
                    // finite field. Include those ghost tiles in clearance.
                    tiles.insert((y, x));
                }
            }
        }
        let mut result = Vec::new();
        let epsilon_area = f64::from(terrain.cell_size()).powi(2) * 1.0e-10;
        for (y, x) in tiles {
            let center = terrain.cell_center(CellCoord::new(x, y));
            let half = terrain.cell_size() * 0.5;
            let min = center - Vec2::new(half, half);
            let max = center + Vec2::new(half, half);
            let clip = rectangle(min, max);
            let old = patches_in_tile(previous, self, CellCoord::new(x, y), &clip);
            for polygon in patches_in_tile(terrain, self, CellCoord::new(x, y), &clip) {
                let mut pieces = vec![polygon];
                for obstacle in &old {
                    pieces = pieces
                        .into_iter()
                        .flat_map(|p| subtract(p, obstacle))
                        .collect();
                    pieces.retain(|p| area(p) > epsilon_area);
                    if pieces.is_empty() {
                        break;
                    }
                }
                result.extend(
                    pieces
                        .into_iter()
                        .filter(|p| area(p) > epsilon_area)
                        .map(|p| {
                            p.into_iter()
                                .map(|v| Vec2::new(v[0] as f32, v[1] as f32))
                                .collect()
                        }),
                );
            }
        }
        result
    }
}

fn patches_in_tile(
    t: &Terrain,
    surface: TerrainSurface,
    cell: CellCoord,
    clip: &Polygon,
) -> Vec<Polygon> {
    let mut result = Vec::new();
    let halo = i32::from(surface != TerrainSurface::Blocks);
    for y in cell.y - halo..=cell.y + halo {
        for x in cell.x - halo..=cell.x + halo {
            let owner = CellCoord::new(x, y);
            if !t
                .cell(owner)
                .is_some_and(|c| c.material != MaterialId::VOID)
            {
                continue;
            }
            let polygons = match surface {
                TerrainSurface::Blocks => {
                    result.push(clip.clone());
                    continue;
                }
                TerrainSurface::Contour => {
                    let mut polygons = vec![surface::cell_polygon(t, owner)];
                    polygons.extend(surface::fills(t, owner).into_iter().map(|p| p.vertices));
                    polygons
                }
                TerrainSurface::Interpolated => surface::interpolated::owned_polygons(t, owner)
                    .into_iter()
                    .map(|p| p.vertices)
                    .collect(),
            };
            for vertices in polygons {
                let mut p: Polygon = vertices.iter().map(|v| [v.x.into(), v.y.into()]).collect();
                for (a, b) in edges(clip) {
                    p = half_plane(&p, a, b, true);
                }
                if !p.is_empty() {
                    result.push(p);
                }
            }
        }
    }
    result
}

fn rectangle(min: Vec2, max: Vec2) -> Polygon {
    vec![
        [min.x.into(), min.y.into()],
        [max.x.into(), min.y.into()],
        [max.x.into(), max.y.into()],
        [min.x.into(), max.y.into()],
    ]
}
fn edges(p: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    p.iter()
        .copied()
        .zip(p.iter().copied().cycle().skip(1))
        .take(p.len())
}
fn cross(a: Point, b: Point, p: Point) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
fn area(p: &[Point]) -> f64 {
    edges(p)
        .map(|(a, b)| a[0] * b[1] - a[1] * b[0])
        .sum::<f64>()
        .abs()
        * 0.5
}
fn half_plane(p: &[Point], a: Point, b: Point, inside: bool) -> Polygon {
    let mut out = Vec::new();
    for (start, end) in edges(p) {
        let ds = cross(a, b, start) * if inside { 1.0 } else { -1.0 };
        let de = cross(a, b, end) * if inside { 1.0 } else { -1.0 };
        if ds >= 0.0 {
            out.push(start);
        }
        if (ds >= 0.0) != (de >= 0.0) {
            let t = ds / (ds - de);
            out.push([
                start[0] + t * (end[0] - start[0]),
                start[1] + t * (end[1] - start[1]),
            ]);
        }
    }
    out.dedup();
    if out.first() == out.last() {
        out.pop();
    }
    if out.len() < 3 {
        out.clear();
    }
    out
}
fn subtract(mut subject: Polygon, clip: &Polygon) -> Vec<Polygon> {
    let mut outside = Vec::new();
    for (a, b) in edges(clip) {
        let piece = half_plane(&subject, a, b, false);
        if !piece.is_empty() {
            outside.push(piece);
        }
        subject = half_plane(&subject, a, b, true);
        if subject.is_empty() {
            break;
        }
    }
    outside
}

#[cfg(test)]
mod tests;
