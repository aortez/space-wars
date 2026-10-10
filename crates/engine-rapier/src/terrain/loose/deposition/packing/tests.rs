use super::*;
use crate::terrain::loose::slumping;
use crate::world::{BodyKind, PhysicsWorldConfig};
use engine_terrain::{DetachedCell, Material};

fn crater(
    surface: TerrainSurface,
    shape: GrainShape,
    angle: f32,
) -> (PhysicsWorld, TerrainFragment, Vec<TerrainGrain>) {
    let terrain = Terrain::generate(
        9,
        9,
        1.0,
        vec![Material {
            id: MaterialId(1),
            hardness: 100,
        }],
        |c| {
            if c.y <= 2 || (c.y == 3 && !(2..=6).contains(&c.x)) {
                MaterialId(1)
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    let mut world = PhysicsWorld::new(PhysicsWorldConfig {
        gravity: Vec2::new(0.0, -10.0).rotate_radians(angle),
        ..Default::default()
    });
    let motion = BodyMotion {
        position: Vec2::new(7.0, 4.0),
        angle,
        linear_velocity: Vec2::ZERO,
        angular_velocity: 0.0,
    };
    let spec = TerrainSpec {
        surface,
        ..Default::default()
    };
    let field = TerrainFragment::insert(
        &mut world,
        PhysicsId::new(1),
        DetachedTerrain {
            terrain,
            parent_offset: Vec2::ZERO,
        },
        motion,
        motion.position,
        spec,
    )
    .unwrap();
    world.set_body_kind(field.assembly.body(), BodyKind::Fixed, true);
    let center = field.terrain.cell_center(CellCoord::new(4, 3));
    let grains = [
        Vec2::new(-0.5, 0.0),
        Vec2::new(0.5, 0.0),
        Vec2::new(0.0, 0.87),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, offset)| {
        TerrainGrain::insert_with_shape(
            &mut world,
            PhysicsId::new(2 + i as u64),
            DetachedCell {
                cell: Cell {
                    material: MaterialId(1),
                    durability: 70 + i as u8,
                },
                cell_size: 1.0,
                parent_offset: center + offset,
            },
            motion,
            motion.position,
            spec,
            shape,
        )
        .unwrap()
    })
    .collect();
    (world, field, grains)
}

// Exhaustively check tiny fixtures, including every subset. This is deliberately
// test-only: it proves whether the bounded runtime missed a legal placement.
fn exhaustive_max(world: &PhysicsWorld, field: &Field<'_>, grains: &[&TerrainGrain]) -> usize {
    assert_eq!(grains.len(), 3);
    let motion = world.motion(field.body).unwrap();
    let positions: Vec<_> = grains
        .iter()
        .map(|g| {
            (world.motion(g.body()).unwrap().position - motion.position)
                .rotate_radians(-motion.angle)
        })
        .collect();
    let candidates: Vec<Vec<_>> = positions
        .iter()
        .map(|&p| {
            let mut options = vec![None];
            for y in 0..field.terrain.height() as i32 {
                for x in 0..field.terrain.width() as i32 {
                    let c = CellCoord::new(x, y);
                    let center = field.terrain.cell_center(c);
                    if field.terrain.cell(c) == Some(Cell::VOID)
                        && p.distance_to(center) <= 1.75 * field.terrain.cell_size()
                        && [0.25, 0.5, 0.75].into_iter().all(|t| {
                            field
                                .geometry
                                .source_cell(field.terrain, p + (center - p) * t)
                                .is_none()
                        })
                    {
                        options.push(Some(c));
                    }
                }
            }
            options
        })
        .collect();
    let mut best = 0;
    for a in &candidates[0] {
        for b in &candidates[1] {
            for c in &candidates[2] {
                let choices = [*a, *b, *c];
                let deposits: Vec<_> = choices
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &c)| {
                        c.map(|coordinate| CellDeposit {
                            coordinate,
                            cell: grains[i].cell(),
                            cell_size: field.terrain.cell_size(),
                        })
                    })
                    .collect();
                if deposits.len() <= best {
                    continue;
                }
                let reach = if deposits.len() == 1 { 1.25 } else { 1.75 };
                if choices.iter().enumerate().any(|(i, c)| {
                    c.is_some_and(|c| {
                        positions[i].distance_to(field.terrain.cell_center(c))
                            > reach * field.terrain.cell_size()
                    })
                }) {
                    continue;
                }
                let mut terrain = field.terrain.clone();
                if terrain.deposit_cells(&deposits).is_err() {
                    continue;
                }
                let changed: Vec<_> = deposits.iter().map(|d| d.coordinate).collect();
                let mut connected = BTreeSet::new();
                loop {
                    let before = connected.len();
                    for c in &changed {
                        if [(0, -1), (-1, 0), (1, 0), (0, 1)]
                            .into_iter()
                            .any(|(dx, dy)| {
                                connected.contains(&(c.y + dy, c.x + dx))
                                    || field
                                        .terrain
                                        .cell(CellCoord::new(c.x + dx, c.y + dy))
                                        .is_some_and(|v| v.material != MaterialId::VOID)
                            })
                        {
                            connected.insert((c.y, c.x));
                        }
                    }
                    if before == connected.len() {
                        break;
                    }
                }
                if connected.len() != deposits.len() {
                    continue;
                }
                if field.repose.as_ref().is_some_and(|r| {
                    changed.iter().any(|c| {
                        slumping::oversteep(&terrain, *c, r.gravity[&(c.y, c.x)], r.degrees)
                    })
                }) {
                    continue;
                }
                let excluded: Vec<_> = std::iter::once(field.body.entity)
                    .chain(
                        choices
                            .iter()
                            .enumerate()
                            .filter_map(|(i, c)| c.map(|_| grains[i].id())),
                    )
                    .collect();
                let patches =
                    field
                        .geometry
                        .surface()
                        .added_surface(field.terrain, &terrain, &changed);
                if world.current_polygons_are_clear(
                    motion.position,
                    motion.angle,
                    &patches,
                    field.groups,
                    &excluded,
                ) {
                    best = deposits.len();
                    if best == grains.len() {
                        return best;
                    }
                }
            }
        }
    }
    best
}

fn actor(world: &mut PhysicsWorld, id: u64, position: Vec2, angle: f32, half: Vec2) -> BodyId {
    let body = BodyId::new(PhysicsId::new(id), BodyRole::PRIMARY);
    assert!(world.insert_body(
        body,
        BodySpec {
            kind: BodyKind::Fixed,
            position,
            angle,
            ..Default::default()
        },
        &[ColliderSpec::cuboid(
            ColliderId::new(body.entity, ColliderRole::PRIMARY, 0),
            half.x,
            half.y
        )]
    ));
    body
}

#[test]
fn small_crater_recovers_every_legal_cell_beside_an_actor() {
    for surface in [
        TerrainSurface::Blocks,
        TerrainSurface::Contour,
        TerrainSurface::Interpolated,
    ] {
        for shape in [GrainShape::Round, GrainShape::Hexagon] {
            for planetary in [false, true] {
                let angle = if planetary { 0.7 } else { 0.0 };
                let (mut world, mut terrain, grains) = crater(surface, shape, angle);
                let motion = world.motion(terrain.assembly.body()).unwrap();
                let center = terrain.terrain.cell_center(CellCoord::new(4, 3));
                let position =
                    motion.position + (center + Vec2::new(1.6, 0.8)).rotate_radians(angle);
                let blocker = actor(&mut world, 99, position, angle, Vec2::new(0.75, 0.35));
                assert!(world.current_cuboid_is_clear(
                    position,
                    angle,
                    Vec2::new(0.75, 0.35),
                    CollisionGroups::ALL,
                    &[blocker.entity]
                ));
                let refs: Vec<_> = grains.iter().collect();
                let view = TerrainBodyMut {
                    terrain: &mut terrain.terrain,
                    geometry: &mut terrain.geometry,
                    assembly: &mut terrain.assembly,
                };
                let field = Field::from(&view).with_repose(
                    &world,
                    &refs,
                    Some(SlumpingConfig::default()),
                    &mut |_, p| {
                        if planetary {
                            (motion.position + Vec2::new(0.0, -30.0).rotate_radians(angle) - p)
                                .normalized()
                                * 10.0
                        } else {
                            Vec2::new(0.0, -10.0)
                        }
                    },
                );
                for present in [true, false] {
                    if !present {
                        world.remove_entity(blocker.entity);
                    }
                    let before = world.snapshot_bytes().unwrap();
                    assert_eq!(exhaustive_max(&world, &field, &refs), 3);
                    let mut trace = Vec::new();
                    let plan = prepare(&world, &field, &refs, Some(&mut trace)).unwrap();
                    if surface != TerrainSurface::Blocks {
                        assert_eq!(
                            plan.accepted.len(),
                            3,
                            "{surface:?}/{shape:?}, planetary={planetary}, actor={present}"
                        );
                    } else {
                        // Ordinary successful block placements retain their
                        // existing order, including valid partial plans.
                        assert!(plan.accepted.len() >= 2);
                    }
                    assert!(
                        trace
                            .iter()
                            .filter(|a| a.stage == "surface additions")
                            .count()
                            <= 8
                    );
                    let last = trace
                        .iter()
                        .find(|a| a.cells == plan.changed && a.patches.iter().all(|p| p.clear))
                        .unwrap();
                    assert!(last.unstable_centers.is_empty());
                    assert!(
                        last.patches
                            .iter()
                            .all(|p| p.clear && p.blockers.is_empty())
                    );
                    for (&i, &c) in plan.accepted.iter().zip(&plan.changed) {
                        assert_eq!(plan.terrain.cell(c), Some(grains[i].cell()));
                    }
                    assert_eq!(world.snapshot_bytes().unwrap(), before);
                }
            }
        }
    }
}

#[test]
fn exhaustive_search_distinguishes_a_cramped_bay_from_clear_space() {
    for shape in [GrainShape::Round, GrainShape::Hexagon] {
        for surface in [
            TerrainSurface::Blocks,
            TerrainSurface::Contour,
            TerrainSurface::Interpolated,
        ] {
            let (mut world, mut terrain, grains) = crater(surface, shape, 0.0);
            let center = world.motion(terrain.assembly.body()).unwrap().position
                + terrain.terrain.cell_center(CellCoord::new(4, 3));
            // Physical proxies fit in this narrow bay, but three whole terrain
            // cells do not. Moving the enclosure does not move a grain.
            let ceiling = actor(
                &mut world,
                99,
                center + Vec2::new(0.0, 1.73),
                0.0,
                Vec2::new(2.0, 0.35),
            );
            actor(
                &mut world,
                100,
                center + Vec2::new(-1.65, 0.5),
                0.0,
                Vec2::new(0.55, 1.0),
            );
            actor(
                &mut world,
                101,
                center + Vec2::new(1.65, 0.5),
                0.0,
                Vec2::new(0.55, 1.0),
            );
            let refs: Vec<_> = grains.iter().collect();
            let view = TerrainBodyMut {
                terrain: &mut terrain.terrain,
                geometry: &mut terrain.geometry,
                assembly: &mut terrain.assembly,
            };
            let field = Field::from(&view);
            assert_eq!(exhaustive_max(&world, &field, &refs), 0);
            assert!(prepare(&world, &field, &refs, None).is_err());
            // Move the enclosing actors without stepping physics. Clearance
            // must use the current transforms, not stale broadphase bounds.
            world.set_pose(ceiling, center + Vec2::new(0.0, 10.0), 0.0, true);
            for id in [100, 101] {
                world.set_pose(
                    BodyId::new(PhysicsId::new(id), BodyRole::PRIMARY),
                    center + Vec2::new(20.0, id as f32),
                    0.0,
                    true,
                );
            }
            assert_eq!(exhaustive_max(&world, &field, &refs), 3);
            let plan = prepare(&world, &field, &refs, None).unwrap();
            assert_eq!(plan.accepted.len(), 3);
        }
    }
}
