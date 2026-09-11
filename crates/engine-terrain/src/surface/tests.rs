use super::*;

fn field(width: u32, height: u32, solid: impl Fn(CellCoord) -> bool) -> Terrain {
    Terrain::generate(
        width,
        height,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |p| {
            if solid(p) {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap()
}

fn covered(t: &Terrain, g: &TerrainGeometry, p: Vec2) -> bool {
    g.chunks().iter().any(|c| {
        c.rectangles.iter().any(|r| {
            let d = p - r.local_center(t);
            let half = r.half_extents(t);
            d.x.abs() <= half.x && d.y.abs() <= half.y
        }) || c.polygons.iter().any(|r| contains(&r.vertices, p))
    })
}

#[test]
fn all_sixteen_corner_patterns_have_expected_area_and_material_mapping() {
    for mask in 0_u32..16 {
        let t = field(2, 2, |p| mask & (1 << (p.y * 2 + p.x)) != 0);
        let g = TerrainGeometry::with_surface(&t, TerrainSurface::Contour);
        assert_eq!(
            g.chunks()
                .iter()
                .map(ChunkGeometry::material_cells)
                .sum::<u64>(),
            u64::from(mask.count_ones())
        );
        let mut filled = 0;
        // Sample the dual square between the four cell centres. Offset samples
        // avoid ties on the diagonals, which do not contribute physical area.
        for y in 0..80 {
            for x in 0..80 {
                let p = Vec2::new(
                    -0.5 + (x as f32 + 0.31) / 80.0,
                    -0.5 + (y as f32 + 0.67) / 80.0,
                );
                let owner = g.source_cell(&t, p);
                assert_eq!(
                    owner.is_some(),
                    covered(&t, &g, p),
                    "mask {mask}, point {p:?}"
                );
                if let Some(owner) = owner {
                    assert!(solid(&t, owner));
                    filled += 1;
                }
            }
        }
        let expected = match mask.count_ones() {
            0 => 0.0,
            1 => 0.125,
            2 if mask == 6 || mask == 9 => 0.25,
            2 => 0.5,
            3 => 0.875,
            _ => 1.0,
        };
        assert!(
            (filled as f32 / 6400.0 - expected).abs() < 0.01,
            "mask {mask}"
        );
    }
}

#[test]
fn isolated_cells_holes_and_edge_connected_bridges_survive() {
    for t in [
        field(1, 1, |_| true),
        field(7, 5, |p| p.x < 2 || p.x > 4 || p.y == 2),
        field(7, 7, |p| p != CellCoord::new(3, 3)),
        field(4, 4, |p| p.x == p.y),
    ] {
        let before = t.hash();
        let g = TerrainGeometry::with_surface(&t, TerrainSurface::Contour);
        for y in 0..t.height() as i32 {
            for x in 0..t.width() as i32 {
                let c = CellCoord::new(x, y);
                assert_eq!(g.source_cell(&t, t.cell_center(c)).is_some(), solid(&t, c));
            }
        }
        assert_eq!(before, t.hash());
        assert!(g.shape_count() > 0);
    }
}

#[test]
fn neighbour_edits_refresh_across_chunk_seams_and_match_fresh_build() {
    let mut t = field(97, 67, |_| true);
    let mut g = TerrainGeometry::with_surface(&t, TerrainSurface::Contour);
    for center in [
        CellCoord::new(32, 32),
        CellCoord::new(63, 31),
        CellCoord::new(96, 66),
    ] {
        t.apply(TerrainEdit {
            brush: Brush::Circle { center, radius: 0 },
            mode: EditMode::Remove,
        })
        .unwrap();
        assert!(!g.is_current(&t));
        let rebuilt = g.refresh(&t);
        assert!(rebuilt.len() > 1);
        assert!(g.is_current(&t));
        let fresh = TerrainGeometry::with_surface(&t, TerrainSurface::Contour);
        for (a, b) in g.chunks().iter().zip(fresh.chunks()) {
            assert_eq!(a.rectangles, b.rectangles);
            assert_eq!(a.polygons, b.polygons);
        }
        assert!(g.refresh(&t).is_empty());
    }
}

#[test]
fn mixed_material_fill_has_one_surviving_owner_and_durability_does_not_move_surface() {
    let mut t = Terrain::generate(
        4,
        4,
        0.5,
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
        |p| {
            if p.x == 2 && p.y == 2 {
                MaterialId::VOID
            } else {
                MaterialId(1 + (p.x % 2) as u8)
            }
        },
    )
    .unwrap();
    let mut g = TerrainGeometry::with_surface(&t, TerrainSurface::Contour);
    let before = g.chunks()[0].polygons.clone();
    for p in &before {
        assert_eq!(t.cell(p.source).unwrap().material, p.material);
        assert!(p.vertices.len() >= 3);
    }
    t.apply(TerrainEdit {
        brush: Brush::Circle {
            center: CellCoord::new(1, 1),
            radius: 4,
        },
        mode: EditMode::Damage(1),
    })
    .unwrap();
    g.refresh(&t);
    assert_eq!(before, g.chunks()[0].polygons);
}

#[test]
fn surviving_attachment_follows_a_concave_corner_after_its_neighbour_is_cut() {
    let mut t = field(3, 3, |c| c != CellCoord::new(1, 1));
    let mut g = TerrainGeometry::with_surface(&t, TerrainSurface::Contour);
    let owner = CellCoord::new(1, 0);
    let point = Vec2::new(-0.2, -0.3);
    let normal = Vec2::new(1.0, 1.0).normalized();
    assert_eq!(g.source_cell(&t, point - normal * 0.01), Some(owner));
    t.apply(TerrainEdit {
        brush: Brush::Circle {
            center: CellCoord::new(0, 1),
            radius: 0,
        },
        mode: EditMode::Remove,
    })
    .unwrap();
    g.refresh(&t);
    assert!(g.source_cell(&t, point).is_none());
    let (p, n) = g.project_source_surface(&t, owner, point, normal).unwrap();
    assert!(p.distance_to(point) > 0.1);
    assert_eq!(g.source_cell(&t, p - n * 0.001), Some(owner));
    assert!(g.source_cell(&t, p + n * 0.001).is_none());
}
