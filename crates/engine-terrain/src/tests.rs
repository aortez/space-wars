use super::*;

#[test]
fn preview_matches_damage_across_chunk_edges_and_clips_to_the_field() {
    let field = Terrain::generate(
        40,
        35,
        0.5,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |p| {
            if (p.x + p.y) % 7 == 0 {
                MaterialId::VOID
            } else {
                MaterialId(1)
            }
        },
    )
    .unwrap();
    for brush in [
        Brush::Circle {
            center: CellCoord::new(0, 0),
            radius: 3,
        },
        Brush::Capsule {
            start: CellCoord::new(29, 28),
            end: CellCoord::new(43, 36),
            radius: 3,
        },
        Brush::Circle {
            center: CellCoord::new(-10, -10),
            radius: 0,
        },
    ] {
        let predicted = field
            .brush_cells(brush)
            .unwrap()
            .map(|(p, _)| p)
            .collect::<Vec<_>>();
        let mut changed = field.clone();
        let result = changed
            .apply(TerrainEdit {
                brush,
                mode: EditMode::Damage(20),
            })
            .unwrap();
        let actual = field
            .cells()
            .iter()
            .zip(changed.cells())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| CellCoord::new(i as i32 % 40, i as i32 / 40))
            .collect::<Vec<_>>();
        assert_eq!(predicted, actual);
        assert_eq!(result.changed_cells as usize, predicted.len());
        assert_eq!(field.revision(), 0, "preview is read-only");
    }
    assert!(
        field
            .brush_cells(Brush::Circle {
                center: CellCoord::new(i32::MAX, 0),
                radius: 1
            })
            .is_err()
    );
}

const ROCK: MaterialId = MaterialId(1);
const ORE: MaterialId = MaterialId(2);

fn field(width: u32, height: u32) -> Terrain {
    Terrain::generate(
        width,
        height,
        0.5,
        vec![
            Material {
                id: ROCK,
                hardness: 10,
            },
            Material {
                id: ORE,
                hardness: 20,
            },
        ],
        |c| {
            if (c.x / 8 + c.y / 8) % 3 == 0 {
                ORE
            } else {
                ROCK
            }
        },
    )
    .unwrap()
}

fn circle(x: i32, y: i32, radius: u32, mode: EditMode) -> TerrainEdit {
    TerrainEdit {
        brush: Brush::Circle {
            center: CellCoord::new(x, y),
            radius,
        },
        mode,
    }
}

#[test]
fn damage_counts_material_only_once_and_marks_four_chunk_corner() {
    let mut terrain = field(65, 65);
    let before = terrain.hash();
    let result = terrain
        .apply(circle(32, 32, 2, EditMode::Damage(5)))
        .unwrap();
    assert_eq!(result.changed_cells, 13);
    assert!(result.removed.is_empty());
    assert_eq!(
        result.dirty_chunks,
        [ChunkId(0), ChunkId(1), ChunkId(3), ChunkId(4)]
    );
    assert_ne!(terrain.hash(), before);
    let result = terrain.apply(circle(32, 32, 2, EditMode::Remove)).unwrap();
    assert_eq!(
        result
            .removed
            .iter()
            .map(|material| material.cells)
            .sum::<u32>(),
        13
    );
    let empty = terrain.apply(circle(32, 32, 2, EditMode::Remove)).unwrap();
    assert_eq!(empty.revision, 2);
    assert_eq!(empty.changed_cells, 0);
    assert!(empty.removed.is_empty());
    assert!(empty.dirty_chunks.is_empty());
    assert!(empty.bounds.is_none());
}

#[test]
fn capsule_is_symmetric_clipped_and_has_round_endpoints() {
    let mut first = field(70, 45);
    let mut second = first.clone();
    let start = CellCoord::new(-8, 2);
    let end = CellCoord::new(67, 35);
    for (terrain, start, end) in [(&mut first, start, end), (&mut second, end, start)] {
        terrain
            .apply(TerrainEdit {
                brush: Brush::Capsule {
                    start,
                    end,
                    radius: 3,
                },
                mode: EditMode::Remove,
            })
            .unwrap();
    }
    assert_eq!(first, second);
    assert_eq!(first.cell(CellCoord::new(69, 35)), Some(Cell::VOID));
    assert_ne!(first.cell(CellCoord::new(67, 40)), Some(Cell::VOID));
    assert_eq!(
        first
            .apply(circle(-100, -100, 1, EditMode::Remove))
            .unwrap()
            .changed_cells,
        0
    );
    assert_eq!(
        first
            .apply(circle(20, 20, 2, EditMode::Damage(0)))
            .unwrap()
            .changed_cells,
        0
    );
    assert!(
        first
            .apply(circle(i32::MAX, 0, u32::MAX, EditMode::Remove))
            .is_err()
    );
}

#[test]
fn serialization_resumes_ordered_edits_and_preserves_two_byte_cells() {
    let mut first = field(97, 66);
    first
        .apply(circle(12, 30, 10, EditMode::Damage(3)))
        .unwrap();
    let bytes = bincode::serialize(&first).unwrap();
    let mut restored: Terrain = bincode::deserialize(&bytes).unwrap();
    assert_eq!(first, restored);
    for index in 0..40 {
        let edit = circle(index * 2, 32, 4, EditMode::Damage(6));
        assert_eq!(first.apply(edit), restored.apply(edit));
        assert_eq!(first.hash(), restored.hash());
    }
    assert_eq!(std::mem::size_of::<Cell>(), 2);
    assert_eq!(first.cell_bytes(), 97 * 66 * 2);
    let mut invalid = first.clone();
    invalid.cells[0] = Cell {
        material: MaterialId(99),
        durability: 1,
    };
    assert!(bincode::deserialize::<Terrain>(&bincode::serialize(&invalid).unwrap()).is_err());
    invalid = first.clone();
    invalid.version = 99;
    assert!(bincode::deserialize::<Terrain>(&bincode::serialize(&invalid).unwrap()).is_err());
}

#[test]
fn rectangle_cover_exactly_matches_material_after_cross_chunk_edits() {
    let mut terrain = field(71, 67);
    let mut geometry = TerrainGeometry::new(&terrain);
    let before = geometry.chunks()[0].rectangles.clone();
    let result = terrain.apply(circle(48, 48, 8, EditMode::Remove)).unwrap();
    assert_eq!(geometry.refresh(&terrain), result.dirty_chunks);
    assert_eq!(before, geometry.chunks()[0].rectangles);
    assert!(geometry.refresh(&terrain).is_empty());
    let mut coverage = vec![MaterialId::VOID; terrain.cells().len()];
    for chunk in geometry.chunks() {
        for rect in &chunk.rectangles {
            for y in rect.min.y as u32..rect.min.y as u32 + rect.height {
                for x in rect.min.x as u32..rect.min.x as u32 + rect.width {
                    let covered = &mut coverage[(y * terrain.width() + x) as usize];
                    assert_eq!(*covered, MaterialId::VOID, "overlapping rectangles");
                    *covered = rect.material;
                }
            }
        }
    }
    assert_eq!(
        coverage,
        terrain
            .cells()
            .iter()
            .map(|cell| cell.material)
            .collect::<Vec<_>>()
    );
    terrain
        .apply(circle(35, 33, 100, EditMode::Remove))
        .unwrap();
    geometry.refresh(&terrain);
    assert_eq!(geometry.rectangle_count(), 0);
}

#[test]
fn cell_coordinates_round_trip_for_even_odd_and_rectangular_fields() {
    for (width, height) in [(1, 1), (32, 65), (81, 81)] {
        let terrain = field(width, height);
        for y in 0..height {
            for x in 0..width {
                let coord = CellCoord::new(x as i32, y as i32);
                assert_eq!(
                    terrain.local_to_cell(terrain.cell_center(coord)),
                    Some(coord)
                );
            }
        }
        assert_eq!(terrain.local_to_cell(Vec2::new(f32::NAN, 0.0)), None);
    }
    assert!(Terrain::generate(0, 2, 0.5, vec![], |_| MaterialId::VOID).is_err());
    assert!(Terrain::generate(5, 2, f32::INFINITY, vec![], |_| MaterialId::VOID).is_err());
    assert!(Terrain::generate(5, 2, 0.5, vec![], |_| ROCK).is_err());
}
