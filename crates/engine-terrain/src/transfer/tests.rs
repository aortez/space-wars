use super::*;

fn field() -> Terrain {
    Terrain::generate(
        70,
        36,
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
        |c| MaterialId(if c.x < 33 { 1 } else { 2 }),
    )
    .unwrap()
}

fn reconstruct(source: &Terrain, pieces: &[DetachedTerrain]) -> Vec<Cell> {
    let mut cells = source.cells.clone();
    for piece in pieces {
        piece.terrain.validate().unwrap();
        for y in 0..piece.terrain.height as i32 {
            for x in 0..piece.terrain.width as i32 {
                let coordinate = CellCoord::new(x, y);
                let cell = piece.terrain.cell(coordinate).unwrap();
                if cell.material == MaterialId::VOID {
                    continue;
                }
                let original = source
                    .local_to_cell(piece.parent_offset + piece.terrain.cell_center(coordinate))
                    .unwrap();
                let destination =
                    &mut cells[(original.y as u32 * source.width + original.x as u32) as usize];
                assert_eq!(*destination, Cell::VOID, "material cannot have two owners");
                *destination = cell;
            }
        }
    }
    cells
}

#[test]
fn transfer_preserves_material_damage_positions_and_dirty_chunks() {
    let mut source = field();
    source
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(33, 32),
                radius: 0,
            },
            mode: EditMode::Damage(37),
        })
        .unwrap();
    let before = source.clone();
    let mut geometry = TerrainGeometry::new(&source);
    let mut coordinates: Vec<_> = (30..34)
        .flat_map(|y| (30..36).map(move |x| CellCoord::new(x, y)))
        .collect();
    coordinates.push(coordinates[0]);
    let pieces = source.extract_cells(&coordinates).unwrap();
    assert_eq!(pieces.len(), 1);
    assert_eq!(reconstruct(&source, &pieces), before.cells);
    assert_eq!(source.revision(), before.revision() + 1);
    assert_eq!(
        geometry.refresh(&source),
        [ChunkId(0), ChunkId(1), ChunkId(3), ChunkId(4)]
    );
    assert_eq!(pieces[0].terrain.revision(), 0);
    assert!(
        pieces[0]
            .terrain
            .cells()
            .iter()
            .any(|cell| cell.durability == 143)
    );
    let after = source.clone();
    assert!(source.extract_cells(&coordinates).unwrap().is_empty());
    assert_eq!(source, after);
}

#[test]
fn disconnected_selections_remain_independent_and_input_order_does_not_matter() {
    let mut source = field();
    let mut other = source.clone();
    let coordinates = [
        CellCoord::new(1, 1),
        CellCoord::new(2, 2),
        CellCoord::new(3, 2),
    ];
    let pieces = source.extract_cells(&coordinates).unwrap();
    let reversed = other
        .extract_cells(&coordinates.into_iter().rev().collect::<Vec<_>>())
        .unwrap();
    assert_eq!(source, other);
    assert_eq!(pieces, reversed);
    assert_eq!(pieces.len(), 2);
    assert_eq!(reconstruct(&source, &pieces), field().cells);
    for mut piece in pieces {
        assert_eq!(piece.terrain.revision(), 0);
        assert!(piece.terrain.detach_disconnected().unwrap().is_empty());
    }
}

#[test]
fn invalid_transfer_and_revision_exhaustion_leave_source_intact() {
    let mut source = field();
    for invalid in [
        CellCoord::new(-1, 0),
        CellCoord::new(70, 0),
        CellCoord::new(0, 36),
    ] {
        let before = source.clone();
        assert!(
            source
                .extract_cells(&[CellCoord::new(1, 1), invalid])
                .is_err()
        );
        assert_eq!(source, before);
    }
    source.revision = u64::MAX;
    let before = source.clone();
    assert!(source.extract_cells(&[CellCoord::new(1, 1)]).is_err());
    assert_eq!(source, before);
    assert!(source.extract_cells(&[]).unwrap().is_empty());
}

#[test]
fn sampled_boundaries_survive_transfer_serialization_and_repeated_extraction() {
    let mut source = field().with_surface_distances(|_| 0.3).unwrap();
    let before = source.clone();
    let mut pieces = source
        .extract_cells(&[CellCoord::new(31, 31), CellCoord::new(32, 31)])
        .unwrap();
    assert_eq!(reconstruct(&source, &pieces), before.cells);
    source.validate().unwrap();
    for piece in &pieces {
        let restored: Terrain =
            bincode::deserialize(&bincode::serialize(&piece.terrain).unwrap()).unwrap();
        assert_eq!(restored, piece.terrain);
        assert!(restored.surface_sample_bytes() > 0);
    }
    let child_coordinate = pieces[0]
        .terrain
        .local_to_cell(before.cell_center(CellCoord::new(31, 31)) - pieces[0].parent_offset)
        .unwrap();
    let child_before = pieces[0].terrain.clone();
    let children = pieces[0]
        .terrain
        .extract_cells(&[child_coordinate])
        .unwrap();
    assert_eq!(
        reconstruct(&pieces[0].terrain, &children),
        child_before.cells
    );
}

#[test]
fn individual_transfer_preserves_damaged_cells_and_updates_the_surface() {
    let mut source = field().with_surface_distances(|_| 0.3).unwrap();
    let damaged = CellCoord::new(33, 32);
    source
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: damaged,
                radius: 0,
            },
            mode: EditMode::Damage(37),
        })
        .unwrap();
    let coordinates = [damaged, CellCoord::new(31, 31), damaged];
    let before = source.clone();
    let mut geometry = TerrainGeometry::new(&source);
    let samples = source.extract_individual_cells(&coordinates).unwrap();
    assert_eq!(samples.len(), 2);
    for (sample, coordinate) in samples.iter().zip([coordinates[1], damaged]) {
        assert_eq!(sample.cell, before.cell(coordinate).unwrap());
        assert_eq!(sample.parent_offset, before.cell_center(coordinate));
        assert_eq!(sample.cell_size, 0.5);
        assert_eq!(source.cell(coordinate), Some(Cell::VOID));
    }
    assert_eq!(samples[1].cell.durability, 143);
    assert_eq!(geometry.refresh(&source), [ChunkId(0), ChunkId(4)]);
    source.validate().unwrap();
    assert_eq!(source.revision(), before.revision() + 1);
    let after = source.clone();
    assert!(
        source
            .extract_individual_cells(&coordinates)
            .unwrap()
            .is_empty()
    );
    assert_eq!(source, after);
}

#[test]
fn individual_transfer_is_atomic_on_invalid_input_or_exhausted_revision() {
    let mut source = field();
    let before = source.clone();
    assert!(
        source
            .extract_individual_cells(&[CellCoord::new(2, 2), CellCoord::new(-1, 0)])
            .is_err()
    );
    assert_eq!(source, before);
    source.revision = u64::MAX;
    let before = source.clone();
    assert!(
        source
            .extract_individual_cells(&[CellCoord::new(2, 2)])
            .is_err()
    );
    assert_eq!(source, before);
    assert!(source.extract_individual_cells(&[]).unwrap().is_empty());
}
