//! Test-only exhaustive local assignments. This search does not use the runtime
//! planner's matching, exclusions, stable frontier, or eight-assessment budget.
//! Clearance and geometry use the same shared primitives as the simulation.
use super::*;

const MAX_ASSIGNMENTS: u128 = 20_000_000;

#[derive(Default, Debug, PartialEq)]
struct Search {
    evaluated: usize,
    connected: usize,
    stable: usize,
    clear: usize,
    best: Vec<(usize, CellCoord)>,
}

fn options(snapshot: &PackingSnapshot, world: &PhysicsWorld) -> Vec<Vec<Option<CellCoord>>> {
    let motion = world.motion(snapshot.body).unwrap();
    let geometry = TerrainGeometry::with_surface(&snapshot.terrain, snapshot.surface);
    snapshot
        .grains
        .iter()
        .map(|g| {
            let local = (world.motion(g.body()).unwrap().position - motion.position)
                .rotate_radians(-motion.angle);
            let mut choices = vec![None];
            // Scan the whole finite destination independently of the planner's
            // candidate cache. The same reach and wall-crossing rules define legality.
            for y in 0..snapshot.terrain.height() as i32 {
                for x in 0..snapshot.terrain.width() as i32 {
                    let c = CellCoord::new(x, y);
                    let center = snapshot.terrain.cell_center(c);
                    if snapshot.terrain.cell(c) == Some(Cell::VOID)
                        && local.distance_to(center) <= snapshot.terrain.cell_size() * 1.75
                        && [0.25, 0.5, 0.75].into_iter().all(|t| {
                            geometry
                                .source_cell(&snapshot.terrain, local + (center - local) * t)
                                .is_none()
                        })
                    {
                        choices.push(Some(c));
                    }
                }
            }
            choices
        })
        .collect()
}

fn assignment_bound(choices: &[Vec<Option<CellCoord>>], limit: usize) -> u128 {
    // Count memberships/assignments by deposit size before duplicate-cell
    // rejection. Saturation makes an oversized query fail before enumeration.
    let mut counts = vec![0_u128; limit + 1];
    counts[0] = 1;
    for c in choices {
        for size in (1..=limit).rev() {
            counts[size] =
                counts[size].saturating_add(counts[size - 1].saturating_mul((c.len() - 1) as u128));
        }
    }
    counts[1..].iter().fold(0, |sum, &n| sum.saturating_add(n))
}

struct Oracle<'a> {
    snapshot: &'a PackingSnapshot,
    world: &'a PhysicsWorld,
    choices: Vec<Vec<Option<CellCoord>>>,
    limit: usize,
    result: Search,
}

impl Oracle<'_> {
    fn run(snapshot: &PackingSnapshot, world: &PhysicsWorld, limit: usize) -> Search {
        assert!((1..=snapshot.grains.len()).contains(&limit));
        let choices = options(snapshot, world);
        let bound = assignment_bound(&choices, limit);
        assert!(
            bound <= MAX_ASSIGNMENTS,
            "{bound} assignments exceed the diagnostic limit; reduce SPACEWARS_PACKING_MAX_GRAINS"
        );
        let mut oracle = Oracle {
            snapshot,
            world,
            choices,
            limit,
            result: Search::default(),
        };
        oracle.visit(0, &mut Vec::new());
        oracle.result
    }

    fn visit(&mut self, index: usize, placed: &mut Vec<(usize, CellCoord)>) {
        if index == self.choices.len() {
            if !placed.is_empty() {
                self.assess(placed);
            }
            return;
        }
        for choice_index in 0..self.choices[index].len() {
            let choice = self.choices[index][choice_index];
            if let Some(c) = choice {
                if placed.len() == self.limit || placed.iter().any(|&(_, other)| other == c) {
                    continue;
                }
                placed.push((index, c));
            }
            self.visit(index + 1, placed);
            if choice.is_some() {
                placed.pop();
            }
        }
    }

    fn assess(&mut self, placed: &[(usize, CellCoord)]) {
        let snapshot = self.snapshot;
        self.result.evaluated += 1;
        let motion = self.world.motion(snapshot.body).unwrap();
        if placed.len() == 1 {
            let (i, c) = placed[0];
            let p = (self
                .world
                .motion(snapshot.grains[i].body())
                .unwrap()
                .position
                - motion.position)
                .rotate_radians(-motion.angle);
            if p.distance_to(snapshot.terrain.cell_center(c)) > snapshot.terrain.cell_size() * 1.25
            {
                return;
            }
        }
        // Connectivity is checked for the complete assignment, without imposing
        // any greedy ordering or requiring its intermediate prefixes to be stable.
        let mut connected = BTreeSet::new();
        loop {
            let before = connected.len();
            for &(_, c) in placed {
                if [(0, -1), (-1, 0), (1, 0), (0, 1)]
                    .into_iter()
                    .any(|(dx, dy)| {
                        connected.contains(&(c.y + dy, c.x + dx))
                            || snapshot
                                .terrain
                                .cell(CellCoord::new(c.x + dx, c.y + dy))
                                .is_some_and(|v| v.material != MaterialId::VOID)
                    })
                {
                    connected.insert((c.y, c.x));
                }
            }
            if connected.len() == before {
                break;
            }
        }
        if connected.len() != placed.len() {
            return;
        }
        self.result.connected += 1;
        let deposits: Vec<_> = placed
            .iter()
            .map(|&(i, coordinate)| engine_terrain::CellDeposit {
                coordinate,
                cell: snapshot.grains[i].cell(),
                cell_size: snapshot.terrain.cell_size(),
            })
            .collect();
        let mut terrain = snapshot.terrain.clone();
        if terrain.deposit_cells(&deposits).is_err() {
            return;
        }
        let changed: Vec<_> = placed.iter().map(|&(_, c)| c).collect();
        if snapshot.repose.as_ref().is_some_and(|r| {
            changed.iter().any(|c| {
                super::super::super::slumping::oversteep(
                    &terrain,
                    *c,
                    r.gravity[&(c.y, c.x)],
                    r.degrees,
                )
            })
        }) {
            return;
        }
        self.result.stable += 1;
        // Omitted grains retain their colliders. Exclude only the destination
        // and the exact grains this particular assignment would retire.
        let excluded: Vec<_> = std::iter::once(snapshot.body.entity)
            .chain(placed.iter().map(|&(i, _)| snapshot.grains[i].id()))
            .collect();
        let patches = snapshot
            .surface
            .added_surface(&snapshot.terrain, &terrain, &changed);
        if !self.world.current_polygons_are_clear(
            motion.position,
            motion.angle,
            &patches,
            snapshot.groups,
            &excluded,
        ) {
            return;
        }
        self.result.clear += 1;
        if placed.len() > self.result.best.len() {
            self.result.best = placed.to_vec();
        }
    }
}

#[test]
fn captured_cave_groups_have_no_legal_deposit_until_the_ship_is_removed() {
    for (bytes, maximum) in [
        (
            include_bytes!("../../../../../tests/fixtures/spacewars-round-roof-small.packing")
                .as_slice(),
            2,
        ),
        (
            include_bytes!("../../../../../tests/fixtures/spacewars-angular-roof-small.packing")
                .as_slice(),
            4,
        ),
    ] {
        let snapshot = PackingSnapshot::from_bytes(bytes).unwrap();
        let mut world = PhysicsWorld::from_snapshot_bytes(&snapshot.physics).unwrap();
        let before = world.snapshot_bytes().unwrap();
        let blocked = Oracle::run(&snapshot, &world, snapshot.grains.len());
        assert!(
            blocked.stable > 0,
            "there must be supported, stable proposals to obstruct"
        );
        assert_eq!(blocked.clear, 0);
        assert!(blocked.best.is_empty());
        assert!(snapshot.inspect().unwrap().accepted.is_empty());
        assert_eq!(world.snapshot_bytes().unwrap(), before);

        assert!(world.remove_entity(PhysicsId::new(10000)));
        let without_actor = world.snapshot_bytes().unwrap();
        let clear = Oracle::run(&snapshot, &world, snapshot.grains.len());
        assert_eq!(clear.best.len(), maximum);
        assert_eq!(
            snapshot
                .inspect_without(&[PhysicsId::new(10000)])
                .unwrap()
                .accepted
                .len(),
            maximum
        );
        assert_eq!(world.snapshot_bytes().unwrap(), without_actor);
        assert_eq!(snapshot.to_bytes().unwrap(), bytes);
    }
}

#[test]
fn limiting_membership_preserves_the_exhaustive_results_for_small_deposits() {
    let snapshot = PackingSnapshot::from_bytes(include_bytes!(
        "../../../../../tests/fixtures/spacewars-round-roof-small.packing"
    ))
    .unwrap();
    let mut world = PhysicsWorld::from_snapshot_bytes(&snapshot.physics).unwrap();
    world.remove_entity(PhysicsId::new(10000));
    let full = Oracle::run(&snapshot, &world, 3);
    let pairs = Oracle::run(&snapshot, &world, 2);
    let singles = Oracle::run(&snapshot, &world, 1);
    assert_eq!(full.best.len(), 2);
    assert_eq!(pairs.best, full.best);
    assert_eq!(pairs.clear, full.clear);
    assert!(pairs.evaluated < full.evaluated);
    // The legal recovery needs a coordinated pair. A singleton must use its
    // smaller reach and continue to treat every other grain as an obstacle.
    assert!(singles.best.is_empty());
}

#[test]
#[ignore = "set SPACEWARS_PACKING_SNAPSHOT to inspect a development capture"]
fn inspect_frozen_packing_exhaustively() {
    let path = std::env::var("SPACEWARS_PACKING_SNAPSHOT").expect("set SPACEWARS_PACKING_SNAPSHOT");
    let snapshot = PackingSnapshot::from_bytes(&std::fs::read(path).unwrap()).unwrap();
    let limit = std::env::var("SPACEWARS_PACKING_MAX_GRAINS")
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(snapshot.grains.len());
    let omitted: Vec<_> = std::env::var("SPACEWARS_PACKING_OMIT_BODIES")
        .unwrap_or_default()
        .split(',')
        .filter(|v| !v.is_empty())
        .map(|v| PhysicsId::new(v.parse().unwrap()))
        .collect();
    // Validate omissions through the same public query interface as the inspector.
    let runtime = snapshot.inspect_without(&omitted).unwrap();
    let mut world = PhysicsWorld::from_snapshot_bytes(&snapshot.physics).unwrap();
    for &id in &omitted {
        assert!(world.remove_entity(id));
    }
    let before = world.snapshot_bytes().unwrap();
    let result = Oracle::run(&snapshot, &world, limit);
    assert_eq!(world.snapshot_bytes().unwrap(), before);
    eprintln!(
        "ORACLE grains={} max_selected={limit} omitted={omitted:?} runtime_accepted={} {result:?}",
        snapshot.grains.len(),
        runtime.accepted.len()
    );
}
