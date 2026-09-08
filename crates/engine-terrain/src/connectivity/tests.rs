use super::*;

const ROCK: MaterialId = MaterialId(1);
const ORE: MaterialId = MaterialId(2);

fn field(width: u32, height: u32, generator: impl FnMut(CellCoord) -> MaterialId) -> Terrain {
    Terrain::generate(
        width,
        height,
        0.5,
        vec![
            Material {
                id: ROCK,
                hardness: 100,
            },
            Material {
                id: ORE,
                hardness: 180,
            },
        ],
        generator,
    )
    .unwrap()
}

fn cut(cell: CellCoord) -> TerrainEdit {
    TerrainEdit {
        brush: Brush::Circle {
            center: cell,
            radius: 0,
        },
        mode: EditMode::Remove,
    }
}

#[test]
fn final_bridge_cut_transfers_exact_cells_and_damage_across_processing_chunks() {
    let mut terrain = field(70, 35, |p| {
        if p.x <= 30 || ((31..=32).contains(&p.x) && p.y == 30) {
            ROCK
        } else if (33..=67).contains(&p.x) && p.y >= 28 {
            if p.x >= 64 { ORE } else { ROCK }
        } else {
            MaterialId::VOID
        }
    });
    let initial = terrain.clone();
    assert!(terrain.detach_disconnected().unwrap().is_empty());
    assert_eq!(terrain, initial);
    terrain
        .apply(TerrainEdit {
            mode: EditMode::Damage(37),
            ..cut(CellCoord::new(65, 32))
        })
        .unwrap();
    let mut geometry = TerrainGeometry::new(&terrain);
    let removed = terrain.apply(cut(CellCoord::new(31, 30))).unwrap();
    assert_eq!(
        removed.removed,
        [RemovedMaterial {
            material: ROCK,
            cells: 1
        }]
    );
    geometry.refresh(&terrain);
    let before = terrain.clone();
    let fragments = terrain.detach_disconnected().unwrap();
    assert_eq!(fragments.len(), 1);
    let fragment = &fragments[0];
    assert_eq!(
        (fragment.terrain.width(), fragment.terrain.height()),
        (36, 7)
    );
    assert_eq!(fragment.terrain.revision(), 0);
    assert_eq!(terrain.revision(), before.revision() + 1);
    assert_eq!(
        geometry.refresh(&terrain),
        [ChunkId(1), ChunkId(2), ChunkId(4), ChunkId(5)]
    );
    let mut reconstructed = terrain.cells.clone();
    for y in 0..fragment.terrain.height() as i32 {
        for x in 0..fragment.terrain.width() as i32 {
            let coord = CellCoord::new(x, y);
            let cell = fragment.terrain.cell(coord).unwrap();
            if cell.material == MaterialId::VOID {
                continue;
            }
            let original = terrain
                .local_to_cell(fragment.parent_offset + fragment.terrain.cell_center(coord))
                .unwrap();
            let destination = &mut reconstructed
                [(original.y as u32 * terrain.width() + original.x as u32) as usize];
            assert_eq!(*destination, Cell::VOID, "no cell can belong to two bodies");
            *destination = cell;
        }
    }
    assert_eq!(reconstructed, before.cells);
    assert_eq!(
        fragment
            .terrain
            .cell(CellCoord::new(33, 4))
            .unwrap()
            .durability,
        143
    );
    let restored: Terrain =
        bincode::deserialize(&bincode::serialize(&fragment.terrain).unwrap()).unwrap();
    assert_eq!(restored, fragment.terrain);
    assert!(terrain.detach_disconnected().unwrap().is_empty());
}

#[test]
fn corners_do_not_connect_and_equal_components_keep_the_first_row_major_cell() {
    let mut terrain = field(4, 4, |p| if p.x == p.y { ROCK } else { MaterialId::VOID });
    let fragments = terrain.detach_disconnected().unwrap();
    assert_eq!(fragments.len(), 3);
    assert_eq!(terrain.cell(CellCoord::new(0, 0)).unwrap().material, ROCK);
    for (index, fragment) in fragments.iter().enumerate() {
        assert_eq!(
            fragment.terrain.cells(),
            &[Cell {
                material: ROCK,
                durability: 100
            }]
        );
        assert_eq!(
            fragment.parent_offset,
            terrain.cell_center(CellCoord::new(index as i32 + 1, index as i32 + 1))
        );
    }
    for solid in [false, true] {
        let mut trivial = field(1, 1, |_| if solid { ROCK } else { MaterialId::VOID });
        assert!(trivial.detach_disconnected().unwrap().is_empty());
        assert_eq!(trivial.revision(), 0);
    }
}

#[test]
fn fragments_can_split_again_and_failed_split_is_atomic() {
    let mut terrain = field(9, 1, |p| if p.x == 5 { MaterialId::VOID } else { ORE });
    let mut fragment = terrain.detach_disconnected().unwrap().remove(0).terrain;
    fragment.apply(cut(CellCoord::new(1, 0))).unwrap();
    let next = fragment.detach_disconnected().unwrap();
    assert_eq!(next.len(), 1);
    assert_eq!(fragment.cells()[0].material, ORE);
    assert_eq!(next[0].terrain.cells()[0].material, ORE);
    let mut exhausted = field(3, 1, |p| if p.x == 1 { MaterialId::VOID } else { ROCK });
    exhausted.revision = u64::MAX;
    let before = exhausted.clone();
    assert!(exhausted.detach_disconnected().is_err());
    assert_eq!(exhausted, before);
}
