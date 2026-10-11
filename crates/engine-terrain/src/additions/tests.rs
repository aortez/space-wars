use super::*;

fn surface_area(g: &TerrainGeometry, t: &Terrain) -> f64 {
    g.chunks()
        .iter()
        .map(|c| {
            c.rectangles
                .iter()
                .map(|r| f64::from(r.width * r.height) * f64::from(t.cell_size()).powi(2))
                .sum::<f64>()
                + c.polygons
                    .iter()
                    .map(|p| {
                        area(
                            &p.vertices
                                .iter()
                                .map(|v| [v.x.into(), v.y.into()])
                                .collect::<Vec<_>>(),
                        )
                    })
                    .sum::<f64>()
        })
        .sum()
}

#[test]
fn additions_match_surface_area_across_corners_holes_and_chunk_seams() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        for mask in 0..16 {
            let mut t = Terrain::generate(
                66,
                35,
                0.5,
                vec![Material {
                    id: MaterialId(1),
                    hardness: 100,
                }],
                |c| {
                    if c.y < 30
                        || (c.y == 30 && (30..34).contains(&c.x) && mask & (1 << (c.x - 30)) != 0)
                    {
                        MaterialId(1)
                    } else {
                        MaterialId::VOID
                    }
                },
            )
            .unwrap();
            if surface == TerrainSurface::Interpolated {
                let original = t.clone();
                t = t
                    .with_surface_distances(|p| {
                        let c = original.local_to_cell(p).unwrap();
                        if original.cell(c).unwrap().material != MaterialId::VOID {
                            0.17
                        } else {
                            -0.09
                        }
                    })
                    .unwrap();
            }
            let original = t.clone();
            let before = TerrainGeometry::with_surface(&t, surface);
            let deposits: Vec<_> = [
                CellCoord::new(31, 31),
                CellCoord::new(32, 31),
                CellCoord::new(32, 32),
            ]
            .into_iter()
            .map(|coordinate| CellDeposit {
                coordinate,
                cell: Cell {
                    material: MaterialId(1),
                    durability: 37,
                },
                cell_size: 0.5,
            })
            .collect();
            t.deposit_cells(&deposits).unwrap();
            let after = TerrainGeometry::with_surface(&t, surface);
            let patches = surface.added_surface(
                &original,
                &t,
                &deposits.iter().map(|d| d.coordinate).collect::<Vec<_>>(),
            );
            let added: f64 = patches
                .iter()
                .map(|p| {
                    area(
                        &p.iter()
                            .map(|v| [v.x.into(), v.y.into()])
                            .collect::<Vec<_>>(),
                    )
                })
                .sum();
            let expected = surface_area(&after, &t) - surface_area(&before, &t);
            assert!(
                (added - expected).abs() < 0.00001,
                "{surface:?} mask {mask}: {added} != {expected}"
            );
            assert!(
                surface
                    .added_surface(&t, &t, &[CellCoord::new(32, 31)])
                    .is_empty()
            );
            // Samples within each patch must be new solid, never old ground.
            for polygon in patches {
                let center =
                    polygon.iter().copied().fold(Vec2::ZERO, |a, b| a + b) / polygon.len() as f32;
                assert!(after.source_cell(&t, center).is_some());
                let patch: Polygon = polygon.iter().map(|v| [v.x.into(), v.y.into()]).collect();
                // Contact-source queries admit a tiny edge tolerance. A sliver
                // from f32 remeshing can fall within it, but no substantial old
                // ground may be reported as growth.
                assert!(
                    before.source_cell(&original, center).is_none()
                        || area(&patch) <= f64::from(t.cell_size()).powi(2) * 1.0e-6
                );
            }
        }
    }
}

#[test]
fn convex_subtraction_preserves_holes_as_nonoverlapping_pieces() {
    let subject = rectangle(Vec2::ZERO, Vec2::new(4.0, 4.0));
    let hole = rectangle(Vec2::new(1.0, 1.0), Vec2::new(3.0, 3.0));
    let result = subtract(subject, &hole);
    assert_eq!(result.len(), 4);
    assert_eq!(result.iter().map(|p| area(p)).sum::<f64>(), 12.0);
    for p in &result {
        let mut intersection = p.clone();
        for (a, b) in edges(&hole) {
            intersection = half_plane(&intersection, a, b, true);
        }
        assert_eq!(area(&intersection), 0.0);
    }
}

#[test]
fn preview_includes_growth_past_the_field_edge_from_interpolated_samples() {
    let before = Terrain::generate(
        4,
        4,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if c == CellCoord::new(1, 0) {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let original = before.clone();
    let before = before
        .with_surface_distances(|p| {
            if original
                .cell(original.local_to_cell(p).unwrap())
                .unwrap()
                .material
                != MaterialId::VOID
            {
                1.0
            } else {
                -0.1
            }
        })
        .unwrap();
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        let mut after = before.clone();
        after
            .deposit_cells(&[CellDeposit {
                coordinate: CellCoord::new(0, 0),
                cell: Cell {
                    material: MaterialId(1),
                    durability: 100,
                },
                cell_size: 1.0,
            }])
            .unwrap();
        let added: f64 = surface
            .added_surface(&before, &after, &[CellCoord::new(0, 0)])
            .iter()
            .map(|p| {
                area(
                    &p.iter()
                        .map(|v| [v.x.into(), v.y.into()])
                        .collect::<Vec<_>>(),
                )
            })
            .sum();
        let expected = surface_area(&TerrainGeometry::with_surface(&after, surface), &after)
            - surface_area(&TerrainGeometry::with_surface(&before, surface), &before);
        assert!(
            (added - expected).abs() < 0.00001,
            "{surface:?}: {added} != {expected}"
        );
    }
}

#[test]
fn alternative_previews_preserve_every_patch_across_overlapping_edits() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        let source = Terrain::generate(
            35,
            6,
            0.5,
            vec![Material {
                id: MaterialId(1),
                hardness: 100,
            }],
            |c| {
                if c.y < 2 {
                    MaterialId(1)
                } else {
                    MaterialId::VOID
                }
            },
        )
        .unwrap();
        let source = source
            .with_surface_distances(|p| if p.y < -0.5 { 0.19 } else { -0.07 })
            .unwrap();
        let mut preview = surface.addition_preview(&source);
        let mut expected = Vec::new();
        let mut alternatives = Vec::new();
        // Revisit overlapping tiles, field edges and a chunk seam in different
        // orders. Only identical reconstruction inputs may reuse a tile.
        for coordinates in [
            vec![(0, 2), (1, 2)],
            vec![(1, 2), (2, 2)],
            vec![(31, 2), (32, 2)],
            vec![(34, 2)],
        ] {
            let mut after = source.clone();
            let edits: Vec<_> = coordinates
                .into_iter()
                .map(|(x, y)| CellDeposit {
                    coordinate: CellCoord::new(x, y),
                    cell: Cell {
                        material: MaterialId(1),
                        durability: 37,
                    },
                    cell_size: 0.5,
                })
                .collect();
            after.deposit_cells(&edits).unwrap();
            let changed: Vec<_> = edits.iter().map(|e| e.coordinate).collect();
            expected.push(surface.added_surface(&source, &after, &changed));
            alternatives.push((after, changed));
        }
        for i in [0, 1, 0, 3, 2, 1, 3, 0] {
            let (after, changed) = &alternatives[i];
            assert_eq!(preview.added_surface(after, changed), expected[i]);
            assert!(preview.added_surface(&source, changed).is_empty());
        }
    }
}

#[test]
fn alternative_previews_track_distance_only_changes_with_the_same_revision() {
    let source = Terrain::generate(
        35,
        7,
        0.5,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if c.y < 3 {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let source = source
        .with_surface_distances(|p| if p.y < -0.25 { 0.19 } else { -0.07 })
        .unwrap();
    let surface = TerrainSurface::Interpolated;
    for deposit in [
        CellCoord::new(0, 3),
        CellCoord::new(32, 3),
        CellCoord::new(34, 3),
    ] {
        let mut base = source.clone();
        base.deposit_cells(&[CellDeposit {
            coordinate: deposit,
            cell: Cell {
                material: MaterialId(1),
                durability: 37,
            },
            cell_size: 0.5,
        }])
        .unwrap();
        let mut preview = surface.addition_preview(&source);
        let expected = surface.added_surface(&source, &base, &[deposit]);
        assert_eq!(preview.added_surface(&base, &[deposit]), expected);
        let mut differences = 0;
        for y in -2..=2 {
            for x in -2..=2 {
                let sample = CellCoord::new(deposit.x + x, deposit.y + y);
                if base.cell(sample).is_none() {
                    continue;
                }
                let after = base
                    .clone()
                    .with_surface_distances(|p| {
                        let c = base.local_to_cell(p).unwrap();
                        if c == sample {
                            base.distance(c) * 0.37
                        } else {
                            base.distance(c)
                        }
                    })
                    .unwrap();
                assert_eq!(after.revision(), base.revision());
                let changed = [deposit, sample];
                let fresh = surface.added_surface(&source, &after, &changed);
                differences +=
                    usize::from(fresh != surface.added_surface(&source, &base, &changed));
                assert_eq!(preview.added_surface(&after, &changed), fresh);
                assert_eq!(preview.added_surface(&base, &[deposit]), expected);
            }
        }
        assert!(
            differences > 0,
            "fixture must alter the reconstructed surface"
        );
    }
}
