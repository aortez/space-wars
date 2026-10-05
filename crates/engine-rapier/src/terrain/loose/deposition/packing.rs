//! Bounded redistribution onto nearby, connected vacant cells. Every grain
//! retains exactly one cell; no material mixing or field expansion is implicit.
use super::*;

pub(super) struct PreparedDeposit {
    pub terrain: Terrain,
    pub geometry: TerrainGeometry,
    pub dirty_chunks: Vec<ChunkId>,
    pub accepted: Vec<usize>,
    pub changed: Vec<CellCoord>,
    pub remaining_reason: Reason,
}

pub(super) fn prepare(
    world: &PhysicsWorld,
    field: &TerrainBodyMut<'_>,
    grains: &[&TerrainGrain],
) -> Result<PreparedDeposit, Reason> {
    let mut active: Vec<_> = (0..grains.len()).collect();
    let mut remaining_reason = Reason::NoRoom;
    loop {
        let selected: Vec<_> = active.iter().map(|&i| grains[i]).collect();
        let (deposits, accepted, reason) = assign(world, field, &selected);
        if deposits.is_empty() {
            return Err(reason);
        }
        if accepted.len() < active.len() {
            // Dropped grains become obstacles again. Replan until exclusions
            // contain exactly the bodies this transaction will retire.
            remaining_reason = reason;
            active = accepted.iter().map(|&i| active[i]).collect();
            continue;
        }
        let mut terrain = field.terrain.clone();
        terrain
            .deposit_cells(&deposits)
            .map_err(|_| Reason::NoRoom)?;
        let changed: Vec<_> = deposits.iter().map(|d| d.coordinate).collect();
        let patches = field
            .geometry
            .surface()
            .added_surface(field.terrain, &terrain, &changed);
        let body = field.assembly.body();
        let motion = world.motion(body).unwrap();
        let excluded: Vec<_> = std::iter::once(body.entity)
            .chain(selected.iter().map(|g| g.id()))
            .collect();
        let clear = world.current_polygons_clearance(
            motion.position,
            motion.angle,
            &patches,
            field.assembly.spec.collision_groups,
            &excluded,
        );
        if clear.iter().any(|clear| !clear) {
            // A contour edit also grows neighboring cells. Remove every
            // placement near an obstructed patch, then recheck the smaller plan.
            // Patches are clipped to cell tiles. Attribute an obstructed tile
            // to edits within the reconstruction halo, using one difference
            // and query pass for the whole group.
            let blocked: Vec<_> = patches
                .iter()
                .zip(clear)
                .filter(|(_, clear)| !clear)
                .map(|(patch, _)| {
                    terrain
                        .local_to_cell(
                            patch.iter().copied().fold(Vec2::ZERO, |a, b| a + b)
                                / patch.len() as f32,
                        )
                        .unwrap()
                })
                .collect();
            let halo = i32::from(field.geometry.surface() != TerrainSurface::Blocks);
            let clear: Vec<_> = deposits
                .iter()
                .enumerate()
                .filter_map(|(i, deposit)| {
                    (!blocked.iter().any(|tile| {
                        (tile.x - deposit.coordinate.x).abs() <= halo
                            && (tile.y - deposit.coordinate.y).abs() <= halo
                    }))
                    .then_some(active[i])
                })
                .collect();
            if clear.is_empty() || clear.len() == active.len() {
                return Err(Reason::Obstructed);
            }
            active = clear;
            remaining_reason = Reason::Obstructed;
            continue;
        }
        let mut geometry = field.geometry.clone();
        let dirty_chunks = geometry.refresh(&terrain);
        return Ok(PreparedDeposit {
            terrain,
            geometry,
            dirty_chunks,
            accepted: active,
            changed,
            remaining_reason,
        });
    }
}

fn assign(
    world: &PhysicsWorld,
    field: &TerrainBodyMut<'_>,
    grains: &[&TerrainGrain],
) -> (Vec<CellDeposit>, Vec<usize>, Reason) {
    let body = field.assembly.body();
    let motion = world.motion(body).unwrap();
    let size = field.terrain.cell_size();
    let excluded: Vec<_> = std::iter::once(body.entity)
        .chain(grains.iter().map(|g| g.id()))
        .collect();
    let mut clearance = BTreeMap::new();
    let mut candidates = Vec::new();
    let mut obstructed = false;
    for grain in grains {
        let local = (world.motion(grain.body()).unwrap().position - motion.position)
            .rotate_radians(-motion.angle);
        let Some(cell) = field.terrain.local_to_cell(local) else {
            candidates.push(Vec::new());
            continue;
        };
        let mut options = Vec::new();
        for y in cell.y - 2..=cell.y + 2 {
            for x in cell.x - 2..=cell.x + 2 {
                let coordinate = CellCoord::new(x, y);
                let center = field.terrain.cell_center(coordinate);
                let distance = local.distance_to(center);
                // Inscribed contact proxies pack more tightly than whole cells.
                // A group needs modest extra room to recover its nominal volume.
                let reach = if grains.len() > 1 { 1.75 } else { 1.25 };
                if field.terrain.cell(coordinate) != Some(Cell::VOID) || distance > size * reach {
                    continue;
                }
                // Redistribution cannot carry material through an existing wall.
                if [0.25, 0.5, 0.75].into_iter().any(|t| {
                    field
                        .geometry
                        .source_cell(field.terrain, local + (center - local) * t)
                        .is_some()
                }) {
                    continue;
                }
                if field.geometry.surface() == TerrainSurface::Blocks {
                    let clear = *clearance.entry((y, x)).or_insert_with(|| {
                        world.current_cuboid_is_clear(
                            motion.position + center.rotate_radians(motion.angle),
                            motion.angle,
                            Vec2::new(size * 0.5, size * 0.5),
                            field.assembly.spec.collision_groups,
                            &excluded,
                        )
                    });
                    if !clear {
                        obstructed = true;
                        continue;
                    }
                }
                options.push((distance, coordinate));
            }
        }
        candidates.push(options);
    }
    let mut cells = BTreeMap::<(i32, i32), Vec<(f32, usize)>>::new();
    for (i, options) in candidates.iter().enumerate() {
        for &(distance, c) in options {
            cells.entry((c.y, c.x)).or_default().push((distance, i));
        }
    }
    for options in cells.values_mut() {
        options.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }
    let mut placed = BTreeSet::new();
    let mut assignments = vec![None; grains.len()];
    loop {
        let mut frontier: Vec<_> = cells
            .iter()
            .filter(|((y, x), _)| {
                !placed.contains(&(*y, *x))
                    && [(0, -1), (-1, 0), (1, 0), (0, 1)]
                        .into_iter()
                        .any(|(dx, dy)| {
                            placed.contains(&(*y + dy, *x + dx))
                                || field
                                    .terrain
                                    .cell(CellCoord::new(*x + dx, *y + dy))
                                    .is_some_and(|c| c.material != MaterialId::VOID)
                        })
            })
            .map(|(&c, options)| (options[0].0, c))
            .collect();
        frontier.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let next = frontier.into_iter().find_map(|(_, c)| {
            match_cell(c, &cells, &mut assignments, &mut BTreeSet::new()).then_some(c)
        });
        let Some(c) = next else { break };
        placed.insert(c);
        if placed.len() == grains.len() {
            break;
        }
    }
    let mut deposits = Vec::new();
    let mut accepted = Vec::new();
    for (i, coordinate) in assignments.into_iter().enumerate() {
        if let Some((y, x)) = coordinate {
            accepted.push(i);
            deposits.push(CellDeposit {
                coordinate: CellCoord::new(x, y),
                cell: grains[i].cell(),
                cell_size: size,
            });
        }
    }
    (
        deposits,
        accepted,
        if obstructed {
            Reason::Obstructed
        } else {
            Reason::NoRoom
        },
    )
}

// An augmenting path can move an earlier grain to another already selected
// cell, leaving a scarce nearby slot for a grain with fewer choices. Selected
// cells always remain face-connected to the field throughout matching.
fn match_cell(
    cell: (i32, i32),
    choices: &BTreeMap<(i32, i32), Vec<(f32, usize)>>,
    assignments: &mut [Option<(i32, i32)>],
    seen: &mut BTreeSet<usize>,
) -> bool {
    for &(_, i) in &choices[&cell] {
        if seen.insert(i)
            && assignments[i]
                .is_none_or(|previous| match_cell(previous, choices, assignments, seen))
        {
            assignments[i] = Some(cell);
            return true;
        }
    }
    false
}
