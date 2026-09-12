use super::*;

fn circle(radius: f32) -> Terrain {
    let side = radius.ceil() as u32 * 2 + 3;
    let half = (side / 2) as i32;
    Terrain::generate(
        side,
        side,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if Vec2::new((c.x - half) as f32, (c.y - half) as f32).length() <= radius {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap()
    .with_surface_distances(|p| radius - p.length())
    .unwrap()
}

fn boundaries(t: &Terrain, g: &TerrainGeometry) -> Vec<(Vec2, Vec2)> {
    let mut edges = Vec::new();
    for chunk in g.chunks() {
        let rectangles = chunk.rectangles.iter().map(|r| {
            let c = r.local_center(t);
            let h = r.half_extents(t);
            [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(x, y)| c + Vec2::new(x * h.x, y * h.y))
                .to_vec()
        });
        for vertices in rectangles.chain(chunk.polygons.iter().map(|p| p.vertices.clone())) {
            for (i, &a) in vertices.iter().enumerate() {
                let b = vertices[(i + 1) % vertices.len()];
                let d = b - a;
                let n = Vec2::new(d.y, -d.x).normalized();
                let pieces = (d.length() / (t.cell_size() * 0.25)).ceil().max(1.0) as usize;
                for j in 0..pieces {
                    let start = a + d * (j as f32 / pieces as f32);
                    let end = a + d * ((j + 1) as f32 / pieces as f32);
                    let m = (start + end) * 0.5;
                    if g.source_cell(t, m + n * 0.0001).is_none() {
                        edges.push((start, end));
                    }
                }
            }
        }
    }
    edges
}

#[test]
fn all_corner_patterns_partition_without_overlap_and_resolve_material() {
    for mask in 0..16 {
        let t = Terrain::generate(
            2,
            2,
            1.0,
            vec![Material {
                id: MaterialId(1),
                hardness: 100,
            }],
            |c| {
                if mask & (1 << (c.y * 2 + c.x)) != 0 {
                    MaterialId(1)
                } else {
                    MaterialId::VOID
                }
            },
        )
        .unwrap()
        .with_surface_distances(|p| {
            let index = ((p.y + 0.5) as usize) * 2 + (p.x + 0.5) as usize;
            [0.003, 0.23, 0.8, 0.44][index] * if mask & (1 << index) != 0 { 1.0 } else { -1.0 }
        })
        .unwrap();
        let g = TerrainGeometry::with_surface(&t, TerrainSurface::Interpolated);
        assert_eq!(
            g.chunks()
                .iter()
                .map(ChunkGeometry::material_cells)
                .sum::<u64>(),
            (mask as u32).count_ones() as u64
        );
        for y in 0..40 {
            for x in 0..40 {
                let p = Vec2::new(
                    -0.5 + (x as f32 + 0.317) / 40.0,
                    -0.5 + (y as f32 + 0.673) / 40.0,
                );
                let covering: Vec<_> = g
                    .chunks()
                    .iter()
                    .flat_map(|c| &c.polygons)
                    .filter(|v| contains(&v.vertices, p))
                    .collect();
                assert!(covering.len() <= 1, "overlap mask={mask} point={p:?}");
                assert_eq!(
                    g.source_cell(&t, p),
                    covering.first().map(|v| v.source),
                    "mask={mask} point={p:?}"
                );
            }
        }
    }
}

#[test]
fn untouched_circle_recovers_radius_and_removes_the_diagonal_crown() {
    let t = circle(59.4);
    for mode in [TerrainSurface::Contour, TerrainSurface::Interpolated] {
        let g = TerrainGeometry::with_surface(&t, mode);
        let errors: Vec<_> = boundaries(&t, &g)
            .into_iter()
            .flat_map(|(a, b)| [a, (a + b) * 0.5, b])
            .map(|p| (p.length() - 59.4).abs())
            .collect();
        let max = errors.into_iter().fold(0.0_f32, f32::max);
        println!("circle {mode:?} maximum boundary radius error={max}");
        if mode == TerrainSurface::Interpolated {
            assert!(max < 0.02, "{max}");
        } else {
            assert!(max > 0.2);
        }
    }
}

#[test]
fn damage_leaves_shape_alone_and_completed_cut_tracks_the_brush_circle() {
    let mut t = circle(59.4);
    let before = t.distances.clone();
    let center = t.local_to_cell(Vec2::ZERO).unwrap();
    let brush = Brush::Circle { center, radius: 6 };
    assert!(
        t.apply(TerrainEdit {
            brush,
            mode: EditMode::Damage(40)
        })
        .unwrap()
        .removed
        .is_empty()
    );
    assert_eq!(before, t.distances);
    let result = t
        .apply(TerrainEdit {
            brush,
            mode: EditMode::Damage(60),
        })
        .unwrap();
    assert_eq!(result.removed[0].cells, 113);
    t.validate().unwrap();
    let g = TerrainGeometry::with_surface(&t, TerrainSurface::Interpolated);
    let edges: Vec<_> = boundaries(&t, &g)
        .into_iter()
        .filter(|(a, _)| a.length() < 8.0)
        .collect();
    assert!(edges.len() > 20);
    let max = edges
        .into_iter()
        .flat_map(|(a, b)| [a, (a + b) * 0.5, b])
        .map(|p| (p.length() - 6.0).abs())
        .fold(0.0_f32, f32::max);
    println!("mined radius-6 circle maximum boundary radius error={max}");
    assert!(max < 0.08, "{max}");
}

#[test]
fn boundary_edits_refresh_neighbour_chunks_and_serialize_with_legacy_records() {
    let legacy = Terrain::generate(
        65,
        65,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |_| MaterialId(1),
    )
    .unwrap();
    // The old eight-field encoding followed by unrelated data must still decode.
    let old = (
        &legacy.version,
        &legacy.width,
        &legacy.height,
        &legacy.cell_size,
        &legacy.materials,
        &legacy.cells,
        &legacy.revision,
        &legacy.chunk_revisions,
    );
    let bytes = bincode::serialize(&(old, 0x12345678_u32)).unwrap();
    let (decoded, tail): (Terrain, u32) = bincode::deserialize(&bytes).unwrap();
    assert_eq!(decoded, legacy);
    assert_eq!(tail, 0x12345678);
    let mut t = legacy.with_surface_distances(|_| 100.0).unwrap();
    let mut g = TerrainGeometry::with_surface(&t, TerrainSurface::Interpolated);
    let result = t
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(32, 32),
                radius: 0,
            },
            mode: EditMode::Remove,
        })
        .unwrap();
    assert_eq!(result.changed_cells, 1);
    assert!(
        result.dirty_chunks.len() >= 4,
        "shape samples across the corner must invalidate"
    );
    assert!(!g.is_current(&t));
    g.refresh(&t);
    let fresh = TerrainGeometry::with_surface(&t, TerrainSurface::Interpolated);
    for (a, b) in g.chunks().iter().zip(fresh.chunks()) {
        assert_eq!(a.rectangles, b.rectangles);
        assert_eq!(a.polygons, b.polygons);
    }
    let bytes = bincode::serialize(&(t.clone(), 0x87654321_u32)).unwrap();
    let (mut restored, tail): (Terrain, u32) = bincode::deserialize(&bytes).unwrap();
    assert_eq!(restored, t);
    assert_eq!(tail, 0x87654321);
    let edit = TerrainEdit {
        brush: Brush::Circle {
            center: CellCoord::new(31, 30),
            radius: 4,
        },
        mode: EditMode::Remove,
    };
    assert_eq!(restored.apply(edit), t.apply(edit));
    assert_eq!(restored.hash(), t.hash());
    restored.distances.as_mut().unwrap()[0] = f32::NAN;
    assert!(bincode::deserialize::<Terrain>(&bincode::serialize(&restored).unwrap()).is_err());
}

#[test]
fn detached_fragment_preserves_shape_samples_and_world_boundary() {
    let mut t = circle(12.4);
    let x = (t.width() / 2) as i32;
    t.apply(TerrainEdit {
        brush: Brush::Capsule {
            start: CellCoord::new(x, 0),
            end: CellCoord::new(x, t.height() as i32 - 1),
            radius: 0,
        },
        mode: EditMode::Remove,
    })
    .unwrap();
    let before = t.clone();
    let fragments = t.detach_disconnected().unwrap();
    assert_eq!(fragments.len(), 1);
    assert!(fragments[0].terrain.surface_sample_bytes() > 0);
    for y in 0..120 {
        for x in 0..120 {
            let p = Vec2::new(
                -14.0 + (x as f32 + 0.317) * 28.0 / 120.0,
                -14.0 + (y as f32 + 0.673) * 28.0 / 120.0,
            );
            let was = source_cell(&before, p).is_some();
            let now = source_cell(&t, p).is_some()
                || fragments
                    .iter()
                    .any(|f| source_cell(&f.terrain, p - f.parent_offset).is_some());
            assert_eq!(was, now, "fragment changed at {p:?}");
        }
    }
    let f = &fragments[0];
    let restored: Terrain = bincode::deserialize(&bincode::serialize(&f.terrain).unwrap()).unwrap();
    assert_eq!(restored.hash(), f.terrain.hash());
}

#[test]
fn harder_material_survives_inside_a_cut_and_serialization_checks_sample_signs() {
    let original = Terrain::generate(
        7,
        7,
        1.0,
        vec![
            Material {
                id: MaterialId(1),
                hardness: 100,
            },
            Material {
                id: MaterialId(2),
                hardness: 180,
            },
        ],
        |c| {
            if c.x == 3 {
                MaterialId(2)
            } else {
                MaterialId(1)
            }
        },
    )
    .unwrap()
    .with_surface_distances(|_| 100.0)
    .unwrap();
    let mut t = original.clone();
    let brush = Brush::Circle {
        center: CellCoord::new(3, 3),
        radius: 2,
    };
    t.apply(TerrainEdit {
        brush,
        mode: EditMode::Damage(100),
    })
    .unwrap();
    t.validate().unwrap();
    for y in 1..=5 {
        let c = CellCoord::new(3, y);
        assert_eq!(t.cell(c).unwrap().durability, 80);
        assert_eq!(source_cell(&t, t.cell_center(c)), Some(c));
    }
    let bytes = serde_json::to_vec(&t).unwrap();
    assert_eq!(serde_json::from_slice::<Terrain>(&bytes).unwrap(), t);
    let mut invalid = serde_json::to_value(&t).unwrap();
    invalid["distances"][0] = serde_json::json!(-0.5);
    assert!(serde_json::from_value::<Terrain>(invalid).is_err());
    t.apply(TerrainEdit {
        brush,
        mode: EditMode::Damage(80),
    })
    .unwrap();
    let mut removed = original;
    removed
        .apply(TerrainEdit {
            brush,
            mode: EditMode::Remove,
        })
        .unwrap();
    assert_eq!(t.hash(), removed.hash());
    let legacy = Terrain::generate(1, 1, 1.0, vec![], |_| MaterialId::VOID).unwrap();
    let json = serde_json::to_value(&legacy).unwrap();
    assert!(json.get("distances").is_none());
    assert_eq!(serde_json::from_value::<Terrain>(json).unwrap(), legacy);
}
