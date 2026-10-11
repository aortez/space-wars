//! Convex patches added by a local material edit. Used to validate growth
//! without treating unchanged supporting terrain as a new obstruction.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

type Point = [f64; 2];
type Polygon = Vec<Point>;

/// Local preview workspace for alternative edits against one immutable field.
/// Reuses old geometry and alternative tiles with identical reconstruction inputs.
/// Drop this workspace after the edit decision to release its tile caches.
pub struct SurfaceAdditionPreview<'a> {
    previous: &'a Terrain,
    surface: TerrainSurface,
    owners: BTreeMap<(i32, i32), Vec<Polygon>>,
    tiles: BTreeMap<(i32, i32), Vec<Polygon>>,
    alternatives: BTreeMap<(i32, i32), Vec<TileAddition>>,
    clipping: PolygonScratch,
}

struct TileAddition {
    samples: [u32; 25],
    patches: Vec<Vec<Vec2>>,
}

impl TerrainSurface {
    pub fn addition_preview(self, previous: &Terrain) -> SurfaceAdditionPreview<'_> {
        SurfaceAdditionPreview {
            previous,
            surface: self,
            owners: BTreeMap::new(),
            tiles: BTreeMap::new(),
            alternatives: BTreeMap::new(),
            clipping: PolygonScratch::default(),
        }
    }

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
        self.addition_preview(previous)
            .added_surface(terrain, changed)
    }
}

impl SurfaceAdditionPreview<'_> {
    /// Same result and patch order as `TerrainSurface::added_surface`. Cache
    /// entries include the full local material/sample stencil, not revisions:
    /// independent alternative fields can have the same revision.
    pub fn added_surface(&mut self, terrain: &Terrain, changed: &[CellCoord]) -> Vec<Vec<Vec2>> {
        let previous = self.previous;
        assert_eq!(
            (previous.width(), previous.height(), previous.cell_size()),
            (terrain.width(), terrain.height(), terrain.cell_size())
        );
        let halo = i32::from(self.surface != TerrainSurface::Blocks);
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
        // Neighboring tiles inspect the same owners. Reconstruct each owner's
        // polygons once per immutable field, retaining the original tile,
        // owner, polygon and clipping order (including very thin patches).
        let mut new_owners = BTreeMap::new();
        let mut pieces = Vec::new();
        let mut remaining = Vec::new();
        let epsilon_area = f64::from(terrain.cell_size()).powi(2) * 1.0e-10;
        for (y, x) in tiles {
            let cell = CellCoord::new(x, y);
            let samples = tile_samples(terrain, self.surface, cell);
            let alternatives = self.alternatives.entry((y, x)).or_default();
            if let Some(cached) = alternatives.iter().find(|a| a.samples == samples) {
                result.extend(cached.patches.iter().cloned());
                continue;
            }
            let center = terrain.cell_center(CellCoord::new(x, y));
            let half = terrain.cell_size() * 0.5;
            let min = center - Vec2::new(half, half);
            let max = center + Vec2::new(half, half);
            let clip = rectangle(min, max);
            let old = self.tiles.entry((y, x)).or_insert_with(|| {
                patches_in_tile(
                    previous,
                    self.surface,
                    CellCoord::new(x, y),
                    &clip,
                    &mut self.owners,
                    &mut self.clipping,
                )
            });
            let mut added = Vec::new();
            for polygon in patches_in_tile(
                terrain,
                self.surface,
                CellCoord::new(x, y),
                &clip,
                &mut new_owners,
                &mut self.clipping,
            ) {
                pieces.push(polygon);
                for obstacle in old.iter() {
                    for p in pieces.drain(..) {
                        self.clipping.subtract_into(p, obstacle, &mut remaining);
                    }
                    std::mem::swap(&mut pieces, &mut remaining);
                    pieces.retain(|p| area(p) > epsilon_area);
                    if pieces.is_empty() {
                        break;
                    }
                }
                added.extend(
                    pieces
                        .drain(..)
                        .filter(|p| area(p) > epsilon_area)
                        .map(|p| {
                            p.into_iter()
                                .map(|v| Vec2::new(v[0] as f32, v[1] as f32))
                                .collect()
                        }),
                );
            }
            result.extend(added.iter().cloned());
            alternatives.push(TileAddition {
                samples,
                patches: added,
            });
        }
        result
    }
}

fn tile_samples(t: &Terrain, surface: TerrainSurface, tile: CellCoord) -> [u32; 25] {
    let mut samples = [0; 25];
    // A tile clips owners at most one cell away. Each owner reads its 3x3
    // neighborhood, so the complete dependency is 5x5, including ghost cells.
    let halo = if surface == TerrainSurface::Blocks {
        0
    } else {
        2
    };
    for y in -halo..=halo {
        for x in -halo..=halo {
            let c = CellCoord::new(tile.x + x, tile.y + y);
            samples[((y + 2) * 5 + x + 2) as usize] = if surface == TerrainSurface::Interpolated {
                t.distance(c).to_bits()
            } else {
                u32::from(t.cell(c).is_some_and(|c| c.material != MaterialId::VOID))
            };
        }
    }
    samples
}

fn patches_in_tile(
    t: &Terrain,
    surface: TerrainSurface,
    cell: CellCoord,
    clip: &Polygon,
    owners: &mut BTreeMap<(i32, i32), Vec<Polygon>>,
    clipping: &mut PolygonScratch,
) -> Vec<Polygon> {
    let mut result = Vec::new();
    let halo = i32::from(surface != TerrainSurface::Blocks);
    for y in cell.y - halo..=cell.y + halo {
        for x in cell.x - halo..=cell.x + halo {
            let owner = CellCoord::new(x, y);
            if t.cell(owner).is_none_or(|c| c.material == MaterialId::VOID) {
                continue;
            }
            if surface == TerrainSurface::Blocks {
                result.push(clip.clone());
                continue;
            }
            let polygons = owners.entry((y, x)).or_insert_with(|| {
                let polygons = match surface {
                    TerrainSurface::Contour => {
                        let mut polygons = vec![surface::cell_polygon(t, owner)];
                        polygons.extend(surface::fills(t, owner).into_iter().map(|p| p.vertices));
                        polygons
                    }
                    TerrainSurface::Interpolated => surface::interpolated::owned_polygons(t, owner)
                        .into_iter()
                        .map(|p| p.vertices)
                        .collect(),
                    TerrainSurface::Blocks => unreachable!(),
                };
                polygons
                    .into_iter()
                    .map(|vertices| vertices.iter().map(|v| [v.x.into(), v.y.into()]).collect())
                    .collect()
            });
            for polygon in polygons {
                let mut p = polygon.clone();
                for (a, b) in edges(clip) {
                    clipping.clip(&mut p, a, b, true);
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
// Scratch belongs to a single preview, never terrain state. Keep the same
// half-plane arithmetic, vertex order and degenerate-polygon handling while
// reusing intermediate storage between edges, tiles and alternative edits.
#[derive(Default)]
struct PolygonScratch {
    clipped: Polygon,
    outside: Polygon,
}

impl PolygonScratch {
    fn clip(&mut self, p: &mut Polygon, a: Point, b: Point, inside: bool) {
        half_plane_into(p, a, b, inside, &mut self.clipped);
        std::mem::swap(p, &mut self.clipped);
    }

    fn subtract_into(&mut self, mut subject: Polygon, clip: &Polygon, outside: &mut Vec<Polygon>) {
        for (a, b) in edges(clip) {
            half_plane_into(&subject, a, b, false, &mut self.outside);
            if !self.outside.is_empty() {
                outside.push(std::mem::take(&mut self.outside));
            }
            self.clip(&mut subject, a, b, true);
            if subject.is_empty() {
                break;
            }
        }
    }
}

fn half_plane_into(p: &[Point], a: Point, b: Point, inside: bool, out: &mut Polygon) {
    out.clear();
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
}

#[cfg(test)]
mod tests;
