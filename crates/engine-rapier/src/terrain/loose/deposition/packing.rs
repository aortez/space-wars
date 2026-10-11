//! Bounded redistribution onto nearby, connected vacant cells. Every grain
//! retains exactly one cell; no material mixing or field expansion is implicit.
use super::*;

pub(super) struct Field<'a> {
    pub terrain: &'a Terrain,
    pub geometry: &'a TerrainGeometry,
    pub body: BodyId,
    pub groups: CollisionGroups,
    pub repose: Option<Repose>,
    pub repacking: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct Repose {
    pub(super) degrees: f32,
    pub(super) gravity: BTreeMap<(i32, i32), Vec2>,
}

impl Field<'_> {
    pub fn with_repose(
        mut self,
        world: &PhysicsWorld,
        grains: &[&TerrainGrain],
        config: Option<SlumpingConfig>,
        gravity: &mut impl FnMut(BodyId, Vec2) -> Vec2,
    ) -> Self {
        if let Some(config) = config {
            let motion = world.motion(self.body).unwrap();
            let mut samples = BTreeMap::new();
            for grain in grains {
                let local = (world.motion(grain.body()).unwrap().position - motion.position)
                    .rotate_radians(-motion.angle);
                if let Some(cell) = self.terrain.local_to_cell(local) {
                    for y in cell.y - 2..=cell.y + 2 {
                        for x in cell.x - 2..=cell.x + 2 {
                            let c = CellCoord::new(x, y);
                            if self.terrain.cell(c) == Some(Cell::VOID) {
                                samples.entry((y, x)).or_insert_with(|| {
                                    gravity(
                                        self.body,
                                        motion.position
                                            + self
                                                .terrain
                                                .cell_center(c)
                                                .rotate_radians(motion.angle),
                                    )
                                    .rotate_radians(-motion.angle)
                                });
                            }
                        }
                    }
                }
            }
            self.repose = Some(Repose {
                degrees: config.repose_degrees,
                gravity: samples,
            });
        }
        self
    }
}
impl<'a> From<&'a TerrainBodyMut<'_>> for Field<'a> {
    fn from(field: &'a TerrainBodyMut<'_>) -> Self {
        Self {
            terrain: field.terrain,
            geometry: field.geometry,
            body: field.assembly.body(),
            groups: field.assembly.spec.collision_groups,
            repose: None,
            repacking: true,
        }
    }
}

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
    field: &Field<'_>,
    grains: &[&TerrainGrain],
    mut trace: Option<&mut Vec<PackingAttempt>>,
) -> Result<PreparedDeposit, Reason> {
    let _span = crate::profile::span("packing");
    // Preserve an ordinary successful placement. Only recover a failed plan,
    // and bound expensive surface reconstructions across all passes.
    let mut remaining_plans = 8;
    let mut assessments = Assessments {
        evaluated: Vec::new(),
        surface: field.geometry.surface().addition_preview(field.terrain),
        cell_obstacles: block_obstacles(world, field, grains),
        candidates: candidates(world, field, grains),
    };
    let result = plan(
        world,
        field,
        grains,
        Strategy::Ordinary,
        &mut remaining_plans,
        &mut assessments,
        trace.as_deref_mut(),
    );
    if result.is_ok() || !field.repacking {
        return result;
    }
    let recovered = plan(
        world,
        field,
        grains,
        Strategy::Broad,
        &mut remaining_plans,
        &mut assessments,
        trace.as_deref_mut(),
    );
    if remaining_plans == 0
        || recovered
            .as_ref()
            .is_ok_and(|p| p.accepted.len() == grains.len())
    {
        return recovered;
    }
    // Keep the established recovery as a fallback. Spend only the remaining
    // surface budget on narrower exclusions, accepting it only if more grains fit.
    let targeted = plan(
        world,
        field,
        grains,
        Strategy::Targeted,
        &mut remaining_plans,
        &mut assessments,
        trace,
    );
    match (recovered, targeted) {
        (Ok(a), Ok(b)) if b.accepted.len() > a.accepted.len() => Ok(b),
        (Ok(a), _) => Ok(a),
        (Err(_), Ok(b)) => Ok(b),
        (Err(a), Err(_)) => Err(a),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Strategy {
    Ordinary,
    Broad,
    Targeted,
}

// Local to one prepare call: terrain, world poses, grain properties, and gravity
// cannot change. Membership must match as well as cells, because omitted grains
// become obstacles. Reuse still consumes a plan slot, preserving search order.
struct Assessment {
    active: Vec<usize>,
    changed: Vec<CellCoord>,
    blocked: Vec<CellCoord>,
    unstable: Vec<CellCoord>,
    attempt: Option<PackingAttempt>,
}

struct Assessments<'a> {
    evaluated: Vec<Assessment>,
    surface: engine_terrain::SurfaceAdditionPreview<'a>,
    // Geometry is immutable throughout prepare. Retain all blockers, then
    // exclude exactly the selected grains for each alternative membership.
    cell_obstacles: BTreeMap<(i32, i32), crate::world::SolidObstacles>,
    candidates: BTreeMap<PhysicsId, Vec<Candidate>>,
}

struct Candidate {
    distance: f32,
    coordinate: CellCoord,
    crosses_wall: bool,
}

fn candidates(
    world: &PhysicsWorld,
    field: &Field<'_>,
    grains: &[&TerrainGrain],
) -> BTreeMap<PhysicsId, Vec<Candidate>> {
    let _span = crate::profile::span("packing_candidates");
    let motion = world.motion(field.body).unwrap();
    grains
        .iter()
        .map(|grain| {
            let local = (world.motion(grain.body()).unwrap().position - motion.position)
                .rotate_radians(-motion.angle);
            let mut options = Vec::new();
            if let Some(cell) = field.terrain.local_to_cell(local) {
                for y in cell.y - 2..=cell.y + 2 {
                    for x in cell.x - 2..=cell.x + 2 {
                        let coordinate = CellCoord::new(x, y);
                        let center = field.terrain.cell_center(coordinate);
                        let distance = local.distance_to(center);
                        if field.terrain.cell(coordinate) == Some(Cell::VOID)
                            && distance <= field.terrain.cell_size() * 1.75
                        {
                            options.push(Candidate {
                                distance,
                                coordinate,
                                crosses_wall: [0.25, 0.5, 0.75].into_iter().any(|t| {
                                    field
                                        .geometry
                                        .source_cell(field.terrain, local + (center - local) * t)
                                        .is_some()
                                }),
                            });
                        }
                    }
                }
            }
            (grain.id(), options)
        })
        .collect()
}

fn block_obstacles(
    world: &PhysicsWorld,
    field: &Field<'_>,
    grains: &[&TerrainGrain],
) -> BTreeMap<(i32, i32), crate::world::SolidObstacles> {
    if field.geometry.surface() != TerrainSurface::Blocks {
        return BTreeMap::new();
    }
    let _span = crate::profile::span("packing_block_clearance");
    let motion = world.motion(field.body).unwrap();
    let size = field.terrain.cell_size();
    let mut cells = BTreeMap::new();
    for grain in grains {
        let local = (world.motion(grain.body()).unwrap().position - motion.position)
            .rotate_radians(-motion.angle);
        let Some(cell) = field.terrain.local_to_cell(local) else {
            continue;
        };
        for y in cell.y - 2..=cell.y + 2 {
            for x in cell.x - 2..=cell.x + 2 {
                let coordinate = CellCoord::new(x, y);
                let center = field.terrain.cell_center(coordinate);
                // Every later membership has this reach or the smaller
                // single-grain reach. Forbidden cells can only shrink the set.
                if field.terrain.cell(coordinate) == Some(Cell::VOID)
                    && local.distance_to(center) <= size * 1.75
                {
                    cells.entry((y, x)).or_insert((
                        motion.position + center.rotate_radians(motion.angle),
                        motion.angle,
                    ));
                }
            }
        }
    }
    let obstacles = world.current_cuboids_obstacles(
        &cells.values().copied().collect::<Vec<_>>(),
        Vec2::new(size * 0.5, size * 0.5),
        field.groups,
        &[field.body.entity],
    );
    cells.into_keys().zip(obstacles).collect()
}

fn plan(
    world: &PhysicsWorld,
    field: &Field<'_>,
    grains: &[&TerrainGrain],
    strategy: Strategy,
    remaining_plans: &mut usize,
    assessments: &mut Assessments<'_>,
    mut trace: Option<&mut Vec<PackingAttempt>>,
) -> Result<PreparedDeposit, Reason> {
    let max_replans = if strategy == Strategy::Ordinary { 0 } else { 3 };
    let mut active: Vec<_> = (0..grains.len()).collect();
    let mut remaining_reason = Reason::NoRoom;
    let mut forbidden = BTreeMap::new();
    let mut replans = 0;
    loop {
        let selected: Vec<_> = active.iter().map(|&i| grains[i]).collect();
        let (deposits, accepted, reason) = assign(
            world,
            field,
            &selected,
            &forbidden,
            max_replans > 0,
            assessments,
            trace.as_deref_mut(),
        );
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
        if *remaining_plans == 0 {
            return Err(Reason::Budget);
        }
        *remaining_plans -= 1;
        let mut terrain = field.terrain.clone();
        terrain
            .deposit_cells(&deposits)
            .map_err(|_| Reason::NoRoom)?;
        let changed: Vec<_> = deposits.iter().map(|d| d.coordinate).collect();
        let index = assessments
            .evaluated
            .iter()
            .position(|a| a.active == active && a.changed == changed);
        let index = index.unwrap_or_else(|| {
            let span = crate::profile::span("packing_surface");
            let patches = assessments.surface.added_surface(&terrain, &changed);
            drop(span);
            let motion = world.motion(field.body).unwrap();
            let excluded: Vec<_> = std::iter::once(field.body.entity)
                .chain(selected.iter().map(|g| g.id()))
                .collect();
            let span = crate::profile::span("packing_clearance");
            let clear = world.current_polygons_clearance(
                motion.position,
                motion.angle,
                &patches,
                field.groups,
                &excluded,
            );
            drop(span);
            let unstable: Vec<_> = changed
                .iter()
                .copied()
                .filter(|c| {
                    field.repose.as_ref().is_some_and(|r| {
                        super::super::slumping::oversteep(
                            &terrain,
                            *c,
                            r.gravity[&(c.y, c.x)],
                            r.degrees,
                        )
                    })
                })
                .collect();
            // Patches are clipped to cell tiles; the reconstruction halo
            // identifies which edits can have contributed to a blocked tile.
            let blocked = patches
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
            let attempt = trace.as_ref().map(|_| {
                let mut attempt = inspection::attempt(
                    world,
                    field,
                    &selected,
                    &changed,
                    &patches,
                    "surface additions",
                );
                attempt.unstable_centers = unstable
                    .iter()
                    .map(|&c| motion.position + terrain.cell_center(c).rotate_radians(motion.angle))
                    .collect();
                attempt
            });
            assessments.evaluated.push(Assessment {
                active: active.clone(),
                changed: changed.clone(),
                blocked,
                unstable,
                attempt,
            });
            assessments.evaluated.len() - 1
        });
        let assessment = &assessments.evaluated[index];
        let blocked = &assessment.blocked;
        let unstable = &assessment.unstable;
        if let Some(ref mut trace) = trace {
            trace.push(assessment.attempt.as_ref().unwrap().clone());
        }
        if !unstable.is_empty() || !blocked.is_empty() {
            let halo = i32::from(field.geometry.surface() != TerrainSurface::Blocks);
            let reason = if blocked.is_empty() {
                Reason::Unstable
            } else {
                Reason::Obstructed
            };
            // Keep the group intact while trying different local cells. Dropping
            // grains immediately turns them into new obstacles and can reject
            // an otherwise packable pile in a cascade of shrinking plans.
            if replans < max_replans {
                for tile in blocked {
                    let mut nearby: Vec<_> = deposits
                        .iter()
                        .filter(|d| {
                            (tile.x - d.coordinate.x).abs() <= halo
                                && (tile.y - d.coordinate.y).abs() <= halo
                        })
                        .collect();
                    if strategy == Strategy::Targeted {
                        // A blocked tile does not prove every edit in its halo
                        // is unusable. Try avoiding only the closest edit, with
                        // coordinate order breaking ties. Recheck the full surface.
                        nearby.sort_by_key(|d| {
                            (
                                (tile.x - d.coordinate.x).pow(2) + (tile.y - d.coordinate.y).pow(2),
                                d.coordinate.y,
                                d.coordinate.x,
                            )
                        });
                        nearby.truncate(1);
                    }
                    for deposit in nearby {
                        forbidden.insert(
                            (deposit.coordinate.y, deposit.coordinate.x),
                            Reason::Obstructed,
                        );
                    }
                }
                for cell in unstable {
                    forbidden.insert((cell.y, cell.x), Reason::Unstable);
                }
                replans += 1;
                remaining_reason = reason;
                continue;
            }
            let clear: Vec<_> = deposits
                .iter()
                .enumerate()
                .filter_map(|(i, deposit)| {
                    (!unstable.contains(&deposit.coordinate)
                        && !blocked.iter().any(|tile| {
                            (tile.x - deposit.coordinate.x).abs() <= halo
                                && (tile.y - deposit.coordinate.y).abs() <= halo
                        }))
                    .then_some(active[i])
                })
                .collect();
            if clear.is_empty() || clear.len() == active.len() {
                return Err(reason);
            }
            active = clear;
            remaining_reason = reason;
            continue;
        }
        let span = crate::profile::span("deposit_geometry");
        let mut geometry = field.geometry.clone();
        let dirty_chunks = geometry.refresh(&terrain);
        drop(span);
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
    field: &Field<'_>,
    grains: &[&TerrainGrain],
    forbidden: &BTreeMap<(i32, i32), Reason>,
    stable_frontier: bool,
    assessments: &Assessments<'_>,
    trace: Option<&mut Vec<PackingAttempt>>,
) -> (Vec<CellDeposit>, Vec<usize>, Reason) {
    let _span = crate::profile::span("packing_assign");
    let body = field.body;
    let size = field.terrain.cell_size();
    let excluded: Vec<_> = std::iter::once(body.entity)
        .chain(grains.iter().map(|g| g.id()))
        .collect();
    let mut clearance = BTreeMap::new();
    let mut candidates = Vec::new();
    let mut reason = Reason::NoRoom;
    let candidate_span = crate::profile::span("packing_candidates");
    for grain in grains {
        let mut options = Vec::new();
        // Poses and the source field cannot change during this query. Reuse
        // geometric candidates, but recheck membership-dependent clearance,
        // forbidden cells and the smaller reach of a single surviving grain.
        for candidate in &assessments.candidates[&grain.id()] {
            let Candidate {
                coordinate,
                distance,
                crosses_wall,
            } = *candidate;
            let (x, y) = (coordinate.x, coordinate.y);
            // Inscribed contact proxies pack more tightly than whole cells.
            // A group needs modest extra room to recover its nominal volume.
            let reach = if grains.len() > 1 { 1.75 } else { 1.25 };
            if distance > size * reach {
                continue;
            }
            if let Some(&blocked) = forbidden.get(&(y, x)) {
                reason = blocked;
                continue;
            }
            // Redistribution cannot carry material through an existing wall.
            if crosses_wall {
                continue;
            }
            if field.geometry.surface() == TerrainSurface::Blocks {
                let clear = *clearance.entry((y, x)).or_insert_with(|| {
                    assessments.cell_obstacles[&(y, x)].is_clear_excluding(&excluded)
                });
                if !clear {
                    reason = Reason::Obstructed;
                    continue;
                }
            }
            options.push((distance, coordinate));
        }
        candidates.push(options);
    }
    drop(candidate_span);
    if let Some(trace) = trace {
        let rejected: Vec<_> = clearance
            .iter()
            .filter_map(|(&(y, x), &clear)| (!clear).then_some(CellCoord::new(x, y)))
            .collect();
        if !rejected.is_empty() {
            let patches: Vec<_> = rejected
                .iter()
                .map(|&c| {
                    let center = field.terrain.cell_center(c);
                    [
                        Vec2::new(-0.5, -0.5),
                        Vec2::new(0.5, -0.5),
                        Vec2::new(0.5, 0.5),
                        Vec2::new(-0.5, 0.5),
                    ]
                    .into_iter()
                    .map(|p| center + p * size)
                    .collect()
                })
                .collect();
            trace.push(inspection::attempt(
                world,
                field,
                grains,
                &rejected,
                &patches,
                "blocked candidate cells",
            ));
        }
    }
    let _span = crate::profile::span("packing_match");
    let mut cells = BTreeMap::<(i32, i32), Vec<(f32, usize)>>::new();
    for (i, options) in candidates.iter().enumerate() {
        for &(distance, c) in options {
            cells.entry((c.y, c.x)).or_default().push((distance, i));
        }
    }
    for options in cells.values_mut() {
        options.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }
    // Priorities are immutable: only connectivity and assignment change as the
    // frontier grows. Keep this order instead of sorting it after every cell.
    let mut ordered: Vec<_> = cells
        .iter()
        .map(|(&c, options)| (options[0].0, c))
        .collect();
    ordered.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut placed = BTreeSet::new();
    let mut assignments = vec![None; grains.len()];
    let mut seen = vec![false; grains.len()];
    loop {
        let next = ordered
            .iter()
            .filter(|(_, (y, x))| {
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
            .find_map(|&(_, c)| {
                // Build the recovery pile from supported cells outward. Nearest-cell
                // matching alone can spend the last grain on an overhanging top,
                // even when a stable vacancy is still available beneath it.
                if stable_frontier
                    && field.repose.as_ref().is_some_and(|repose| {
                        super::super::slumping::oversteep_with(
                            field.terrain,
                            CellCoord::new(c.1, c.0),
                            repose.gravity[&c],
                            repose.degrees,
                            |neighbor| {
                                let key = (neighbor.y, neighbor.x);
                                key != c
                                    && !placed.contains(&key)
                                    && field.terrain.cell(neighbor) == Some(Cell::VOID)
                            },
                        )
                    })
                {
                    reason = Reason::Unstable;
                    return None;
                }
                seen.fill(false);
                match_cell(c, &cells, &mut assignments, &mut seen).then_some(c)
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
    (deposits, accepted, reason)
}

// An augmenting path can move an earlier grain to another already selected
// cell, leaving a scarce nearby slot for a grain with fewer choices. Selected
// cells always remain face-connected to the field throughout matching.
fn match_cell(
    cell: (i32, i32),
    choices: &BTreeMap<(i32, i32), Vec<(f32, usize)>>,
    assignments: &mut [Option<(i32, i32)>],
    seen: &mut [bool],
) -> bool {
    for &(_, i) in &choices[&cell] {
        if seen[i] {
            continue;
        }
        seen[i] = true;
        if assignments[i].is_none_or(|previous| match_cell(previous, choices, assignments, seen)) {
            assignments[i] = Some(cell);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
