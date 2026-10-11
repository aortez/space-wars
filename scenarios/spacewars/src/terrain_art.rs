//! Shared terrain artwork for ordinary rendering and the renderer lab. Geometry
//! identity/lifetime belongs to the caller; no GPU or cache handles live here.
use engine_common::PolygonGeometry;
use engine_common::{Fill, RenderColor, RenderPoint, RenderPolygon, Stroke};
use engine_core::Vec2;
use engine_terrain::{ChunkGeometry, MaterialId, Terrain};
use std::{collections::BTreeMap, sync::Arc};

/// Bounded source-art cache, owned by one scenario, shared by its camera views.
/// It contains no tessellation, GPU state or persistent simulation data.
#[derive(Debug, Clone, Default)]
pub(super) struct Cache {
    entries: BTreeMap<usize, Entry>,
    clock: u64,
    bytes: usize,
}
#[derive(Debug, Clone)]
struct Entry {
    _identity: Arc<()>,
    art: Arc<PolygonGeometry>,
    used: u64,
}
impl Cache {
    pub fn get(&mut self, field: &Terrain, chunk: &ChunkGeometry) -> Option<Arc<PolygonGeometry>> {
        const MAX_ENTRIES: usize = 128;
        const MAX_BYTES: usize = 4 * 1024 * 1024;
        self.clock += 1;
        let key = Arc::as_ptr(chunk.identity()) as usize;
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = self.clock;
            return Some(entry.art.clone());
        }
        let art =
            Arc::new(PolygonGeometry::new(polygons(field, chunk, Vec2::ZERO, 0.0).collect()).ok()?);
        let bytes = art.payload_bytes();
        if bytes > MAX_BYTES {
            return None;
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_BYTES {
            let victim = *self.entries.iter().min_by_key(|(_, e)| e.used).unwrap().0;
            self.bytes -= self.entries.remove(&victim).unwrap().art.payload_bytes();
        }
        self.entries.insert(
            key,
            Entry {
                _identity: chunk.identity().clone(),
                art: art.clone(),
                used: self.clock,
            },
        );
        self.bytes += bytes;
        Some(art)
    }
}

/// Preserve rectangle/polygon order and logical-pixel outlines. Passing a zero
/// pose returns local artwork suitable for an immutable, revision-owned batch.
pub fn polygons<'a>(
    field: &'a Terrain,
    chunk: &'a ChunkGeometry,
    position: Vec2,
    angle: f32,
) -> impl Iterator<Item = RenderPolygon> + 'a {
    polygons_with_pool(field, chunk, position, angle, None)
}

pub(super) fn polygons_with_pool<'a>(
    field: &'a Terrain,
    chunk: &'a ChunkGeometry,
    position: Vec2,
    angle: f32,
    mut pool: Option<&'a mut engine_common::RenderPointPool>,
) -> impl Iterator<Item = RenderPolygon> + 'a {
    (0..chunk.rectangles.len() + chunk.polygons.len()).map(move |index| {
        if let Some(r) = chunk.rectangles.get(index) {
            let c = r.local_center(field);
            let h = r.half_extents(field);
            polygon(
                [(-h.x, -h.y), (h.x, -h.y), (h.x, h.y), (-h.x, h.y)]
                    .into_iter()
                    .map(move |(x, y)| c + Vec2::new(x, y)),
                r.material,
                position,
                angle,
                pool.as_deref_mut(),
            )
        } else {
            let p = &chunk.polygons[index - chunk.rectangles.len()];
            polygon(
                p.vertices.iter().copied(),
                p.material,
                position,
                angle,
                pool.as_deref_mut(),
            )
        }
    })
}

fn polygon(
    points: impl Iterator<Item = Vec2>,
    material: MaterialId,
    position: Vec2,
    angle: f32,
    pool: Option<&mut engine_common::RenderPointPool>,
) -> RenderPolygon {
    let color = if material == MaterialId(2) {
        RenderColor::rgb(0.53, 0.49, 0.25)
    } else {
        RenderColor::rgb(0.22, 0.36, 0.45)
    };
    let points = points.map(|p| {
        let p = position + p.rotate_radians(angle);
        RenderPoint::new(p.x, p.y)
    });
    RenderPolygon {
        points: match pool {
            Some(pool) => pool.collect(points),
            None => points.collect(),
        },
        fill: Some(Fill::new(color)),
        stroke: Some(Stroke::new(color, 0.75)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_common::Scenario;
    use engine_terrain::{
        Brush, CellCoord, EditMode, TerrainEdit, TerrainGeometry, TerrainSurface,
    };

    #[test]
    fn retained_art_tracks_neighbor_rebuilds_and_world_replacement() {
        let mut field = crate::SpacewarsScenario::init_terrain_fixture(42)
            .planet_terrain(0)
            .unwrap()
            .clone();
        let mut geometry = TerrainGeometry::with_surface(&field, TerrainSurface::Contour);
        let mut cache = Cache::default();
        let before: Vec<_> = geometry
            .chunks()
            .iter()
            .map(|c| cache.get(&field, c))
            .collect();
        for (c, old) in geometry.chunks().iter().zip(&before) {
            if let Some(old) = old {
                assert!(Arc::ptr_eq(old, &cache.get(&field, c).unwrap()));
            }
        }
        field
            .apply(TerrainEdit {
                brush: Brush::Circle {
                    center: CellCoord::new(64, 64),
                    radius: 2,
                },
                mode: EditMode::Remove,
            })
            .unwrap();
        let rebuilt = geometry.refresh(&field);
        assert!(!rebuilt.is_empty());
        let mut changed = 0;
        for (c, old) in geometry.chunks().iter().zip(&before) {
            if let (Some(old), Some(new)) = (old, cache.get(&field, c)) {
                assert_eq!(!Arc::ptr_eq(old, &new), rebuilt.contains(&c.id));
                changed += usize::from(!Arc::ptr_eq(old, &new));
            }
        }
        assert!(changed > 0);
        // Same field and revision values, different owning geometry lifetime.
        let replacement = TerrainGeometry::with_surface(&field, TerrainSurface::Contour);
        for (a, b) in geometry.chunks().iter().zip(replacement.chunks()) {
            if let (Some(a), Some(b)) = (cache.get(&field, a), cache.get(&field, b)) {
                assert!(!Arc::ptr_eq(&a, &b));
            }
        }
        assert!(cache.entries.len() <= 128);
        assert!(cache.bytes <= 4 * 1024 * 1024);
    }

    #[test]
    fn source_hints_do_not_change_culled_world_primitives_or_state_hash() {
        let state = crate::SpacewarsScenario::init_terrain_fixture(42);
        let material = &state.terrain.planets[&0];
        let before = crate::SpacewarsScenario::observe(&state).payload;
        let mut art = Cache::default();
        for angle in [0.0, 0.8, -1.7] {
            let camera = engine_common::Camera2::new(RenderPoint::new(100.0, 150.0), 160.0);
            let mut ordinary = engine_common::RenderFrame::new(camera);
            let mut hinted = ordinary.clone();
            let position = Vec2::new(130.0, 180.0);
            let view = Some(camera.world_bounds(0.7));
            crate::terrain::render_body_in_view(
                &mut ordinary,
                &material.field,
                &material.geometry,
                position,
                angle,
                view,
            );
            crate::terrain::render_body_with_hints(
                &mut hinted,
                &material.field,
                &material.geometry,
                position,
                angle,
                view,
                Some(&mut art),
            );
            assert_eq!(ordinary, hinted);
            assert!(!hinted.geometry_hints.is_empty());
        }
        assert_eq!(before, crate::SpacewarsScenario::observe(&state).payload);
    }

    #[test]
    fn shared_artwork_preserves_original_points_styles_and_order() {
        let mut field = crate::SpacewarsScenario::init_terrain_fixture(42)
            .planet_terrain(0)
            .unwrap()
            .clone();
        for edited in [false, true] {
            if edited {
                field
                    .apply(TerrainEdit {
                        brush: Brush::Circle {
                            center: CellCoord::new(64, 64),
                            radius: 9,
                        },
                        mode: EditMode::Remove,
                    })
                    .unwrap();
            }
            for surface in [
                TerrainSurface::Blocks,
                TerrainSurface::Contour,
                TerrainSurface::Interpolated,
            ] {
                let geometry = TerrainGeometry::with_surface(&field, surface);
                for angle in [0., 0.7, -1.2] {
                    let position = Vec2::new(123.4, -567.8);
                    for chunk in geometry.chunks() {
                        // Independent copy of the pre-extraction producer: catch
                        // any world-transform rounding or fill/stroke drift.
                        let mut expected = Vec::new();
                        let mut push = |points: Vec<RenderPoint>, material| {
                            let color = if material == MaterialId(2) {
                                RenderColor::rgb(0.53, 0.49, 0.25)
                            } else {
                                RenderColor::rgb(0.22, 0.36, 0.45)
                            };
                            expected.push(RenderPolygon {
                                points,
                                fill: Some(Fill::new(color)),
                                stroke: Some(Stroke::new(color, 0.75)),
                            });
                        };
                        for r in &chunk.rectangles {
                            let center = r.local_center(&field);
                            let half = r.half_extents(&field);
                            push(
                                [
                                    (-half.x, -half.y),
                                    (half.x, -half.y),
                                    (half.x, half.y),
                                    (-half.x, half.y),
                                ]
                                .map(|(x, y)| {
                                    let p =
                                        position + (center + Vec2::new(x, y)).rotate_radians(angle);
                                    RenderPoint::new(p.x, p.y)
                                })
                                .to_vec(),
                                r.material,
                            );
                        }
                        for p in &chunk.polygons {
                            push(
                                p.vertices
                                    .iter()
                                    .map(|p| {
                                        let p = position + p.rotate_radians(angle);
                                        RenderPoint::new(p.x, p.y)
                                    })
                                    .collect(),
                                p.material,
                            );
                        }
                        assert_eq!(
                            polygons(&field, chunk, position, angle).collect::<Vec<_>>(),
                            expected
                        );
                    }
                }
            }
        }
    }
}
