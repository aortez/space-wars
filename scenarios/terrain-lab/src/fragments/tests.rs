use super::*;
use crate::tests::step;
use crate::{
    FIXED_HZ, MiningControls, MiningInventory, MiningTool, PendingEdit, TerrainLabAction,
    TerrainLabConfig, TerrainLabScenario,
};
use engine_common::Scenario;
use engine_rapier::world::BodyKind;
use engine_terrain::{Brush, CellCoord, EditMode, Material, TerrainEdit};

fn fixture(gravity: f32) -> TerrainLabState {
    let mut state = TerrainLabScenario::init(
        TerrainLabConfig {
            radius: 7.5,
            angular_velocity: 0.0,
            orbit_radius: 0.0,
            gravity_acceleration: gravity,
            ..Default::default()
        },
        42,
    );
    state.physics.remove_entity(PLANET_ID);
    state.terrain = Terrain::generate(
        31,
        31,
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
        |p| {
            if p.y <= 15 || (p.x == 3 && p.y <= 24) || ((4..=9).contains(&p.x) && p.y == 24) {
                ROCK
            } else if (10..=14).contains(&p.x) && (23..=25).contains(&p.y) {
                if p.x >= 12 { ORE } else { ROCK }
            } else {
                MaterialId::VOID
            }
        },
    )
    .unwrap();
    state
        .terrain
        .apply(TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(13, 25),
                radius: 0,
            },
            mode: EditMode::Damage(37),
        })
        .unwrap();
    state.geometry = TerrainGeometry::new(&state.terrain);
    state.terrain_hash = state.terrain.hash();
    state.terrain_assembly = TerrainAssembly::insert(
        &mut state.physics,
        PLANET_ID,
        BodySpec {
            kind: BodyKind::KinematicPosition,
            ..Default::default()
        },
        &state.terrain,
        &state.geometry,
        TerrainSpec::default(),
    )
    .unwrap();
    state
        .physics
        .set_pose(state.spaceling.body(), Vec2::new(30.0, 20.0), 0.0, true);
    state.physics.step(1.0 / FIXED_HZ as f32);
    state
}

fn release(state: &mut TerrainLabState) {
    state.pending_edits.push(
        TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(9, 24),
                radius: 0,
            },
            mode: EditMode::Remove,
        }
        .into(),
    );
    state.commit_edits();
}

fn material_counts(state: &TerrainLabState) -> [usize; 2] {
    let mut counts = [0; 2];
    for (terrain, _, _, _) in state.terrain_bodies() {
        for cell in terrain.cells() {
            match cell.material {
                ROCK => counts[0] += 1,
                ORE => counts[1] += 1,
                _ => {}
            }
        }
    }
    counts
}

#[test]
fn mining_the_bridge_collects_only_the_cut_and_leaves_the_detached_ore_intact() {
    let mut state = fixture(0.0);
    state.select_tool(MiningTool::Precision);
    state.config.mining_tools[MiningTool::Precision as usize].damage = 100;
    state
        .physics
        .set_pose(state.spaceling.body(), Vec2::new(-3.0, 2.0), 0.0, true);
    state.physics.step(1.0 / FIXED_HZ as f32);
    step(
        &mut state,
        &[TerrainLabAction::Mining(MiningControls {
            held: true,
            aim: Vec2::Y,
            ..Default::default()
        })
        .encode()],
    );
    assert_eq!(state.fragments.len(), 1);
    assert_eq!(
        state.recovered,
        MiningInventory {
            rock_cells: 1,
            ore_cells: 0
        }
    );
    assert_eq!(
        state.fragments[0]
            .terrain
            .cell(CellCoord::new(3, 2))
            .unwrap()
            .durability,
        143
    );
    assert_eq!(state.last_edit.removed_cells, 1);
    assert_eq!(state.last_edit.detached_cells, 15);
}

#[test]
fn last_connection_creates_a_body_with_the_same_world_cells_and_point_velocities() {
    let mut state = fixture(0.0);
    let body = state.terrain_assembly.body();
    state
        .physics
        .set_pose(body, Vec2::new(7.0, -3.0), 0.63, true);
    state
        .physics
        .set_velocity(body, Vec2::new(2.0, 3.0), 0.8, true);
    let source = state.terrain.clone();
    let motion = state.planet_motion();
    let point = state.local_to_world(source.cell_center(CellCoord::new(13, 25)));
    let point_velocity = state.physics.velocity_at_point(body, point).unwrap();
    let before = material_counts(&state);
    release(&mut state);
    assert_eq!(state.fragments.len(), 1);
    assert_eq!(state.removed_cells, 1);
    assert_eq!(state.last_edit.detached_cells, 15);
    assert_eq!(state.recovered, MiningInventory::default());
    assert_eq!(material_counts(&state), [before[0] - 1, before[1]]);
    let fragment = &state.fragments[0];
    assert_eq!(fragment.id, PhysicsId::new(3));
    assert_eq!(
        fragment
            .terrain
            .cell(CellCoord::new(3, 2))
            .unwrap()
            .durability,
        143
    );
    let fragment_motion = state.physics.motion(fragment.assembly.body()).unwrap();
    assert!((fragment_motion.angle - motion.angle).abs() < 0.000001);
    assert_eq!(fragment_motion.angular_velocity, motion.angular_velocity);
    assert!(
        state
            .physics
            .velocity_at_point(fragment.assembly.body(), point)
            .unwrap()
            .distance_to(point_velocity)
            < 0.0001
    );
    assert!(
        (state.physics.body_mass(fragment.assembly.body()).unwrap() - 15.0 * 0.25).abs() < 0.0001
    );
    for y in 0..3 {
        for x in 0..5 {
            let actual = fragment_motion.position
                + fragment
                    .terrain
                    .cell_center(CellCoord::new(x, y))
                    .rotate_radians(fragment_motion.angle);
            let expected = motion.position
                + source
                    .cell_center(CellCoord::new(x + 10, y + 23))
                    .rotate_radians(motion.angle);
            assert!(actual.distance_to(expected) < 0.00001);
            assert_eq!(
                state
                    .terrain
                    .cell(CellCoord::new(x + 10, y + 23))
                    .unwrap()
                    .material,
                MaterialId::VOID
            );
        }
    }
}

#[test]
fn detached_piece_falls_collides_with_the_planet_and_resumes_from_a_checkpoint() {
    let mut first = fixture(8.0);
    // Keep the floor intact while testing rigid support and checkpoint motion.
    // Impact damage and collapsing floors have separate end-to-end regressions.
    first.config.impacts.enabled = false;
    release(&mut first);
    let body = first.fragments[0].assembly.body();
    let before = first.physics.motion(body).unwrap().position;
    for _ in 0..12 {
        step(&mut first, &[]);
    }
    assert!(first.physics.motion(body).unwrap().position.y < before.y - 0.1);
    let mut restored = first.clone();
    for _ in 0..240 {
        step(&mut first, &[]);
        step(&mut restored, &[]);
        assert_eq!(
            TerrainLabScenario::observe(&first).payload,
            TerrainLabScenario::observe(&restored).payload
        );
    }
    let after = first.physics.motion(body).unwrap();
    assert!(
        after.position.y < before.y - 2.0,
        "piece must fall: {after:?}"
    );
    assert!(
        after.position.y > 0.8,
        "piece must land on the floor: {after:?}"
    );
    assert!(
        after.linear_velocity.length() < 0.5,
        "piece should settle: {after:?}"
    );
    assert_eq!(first.recovered, MiningInventory::default());
    assert_eq!(first.fragments[0].terrain, restored.fragments[0].terrain);
    first.physics.set_pose(
        first.spaceling.body(),
        after.position + Vec2::new(0.0, 1.8),
        0.0,
        true,
    );
    first
        .physics
        .set_velocity(first.spaceling.body(), Vec2::ZERO, 0.0, true);
    for _ in 0..120 {
        step(&mut first, &[]);
    }
    assert_eq!(
        first
            .spaceling_snapshot()
            .support
            .map(|s| s.collider.entity),
        Some(body.entity),
        "detached terrain must support the character"
    );
}

#[test]
fn a_moving_fragment_splits_again_without_losing_material_or_resetting_motion() {
    let mut state = fixture(0.0);
    release(&mut state);
    let body = state.fragments[0].assembly.body();
    state
        .physics
        .set_pose(body, Vec2::new(30.0, 10.0), -0.5, true);
    state
        .physics
        .set_velocity(body, Vec2::new(-3.0, 1.0), 1.1, true);
    let before = material_counts(&state);
    let before_motion = state.physics.motion(body).unwrap();
    let before_com = state.physics.center_of_mass(body).unwrap();
    state.pending_edits.push(PendingEdit {
        body: body.entity,
        cause: EditCause::Debug,
        edit: TerrainEdit {
            brush: Brush::Capsule {
                start: CellCoord::new(2, 0),
                end: CellCoord::new(2, 2),
                radius: 0,
            },
            mode: EditMode::Remove,
        },
    });
    state.commit_edits();
    assert_eq!(state.fragments.len(), 2);
    assert_eq!(material_counts(&state), [before[0], before[1] - 3]);
    assert_eq!(state.recovered, MiningInventory::default());
    assert_eq!(state.fragments[0].id, body.entity);
    assert_eq!(state.fragments[1].id, PhysicsId::new(4));
    for fragment in &state.fragments {
        let body = fragment.assembly.body();
        let offset = state.physics.center_of_mass(body).unwrap() - before_com;
        let expected = before_motion.linear_velocity
            + Vec2::new(-offset.y, offset.x) * before_motion.angular_velocity;
        let motion = state.physics.motion(body).unwrap();
        assert!(motion.linear_velocity.distance_to(expected) < 0.0001);
        assert_eq!(motion.angular_velocity, before_motion.angular_velocity);
        assert!((state.physics.body_mass(body).unwrap() - 6.0 * 0.25).abs() < 0.0001);
    }
    // The two pieces must also collide with each other in the canonical world.
    let bodies = [
        state.fragments[0].assembly.body(),
        state.fragments[1].assembly.body(),
    ];
    for (body, direction) in bodies.into_iter().zip([-1.0, 1.0]) {
        state
            .physics
            .set_pose(body, Vec2::new(direction * 3.0, 40.0), 0.0, true);
        state
            .physics
            .set_velocity(body, Vec2::new(-direction * 4.0, 0.0), 0.0, true);
    }
    for _ in 0..120 {
        step(&mut state, &[]);
    }
    let a = state.physics.center_of_mass(bodies[0]).unwrap();
    let b = state.physics.center_of_mass(bodies[1]).unwrap();
    assert!(
        b.x - a.x >= 0.99,
        "fragments must not pass through one another"
    );
    for body in bodies {
        assert!(state.physics.motion(body).unwrap().linear_velocity.length() < 0.1);
    }
}

#[test]
fn beam_and_preview_follow_a_rotated_fragment_and_mining_removes_its_last_body() {
    for angle in [0.0, 0.73, -1.2] {
        let mut state = fixture(0.0);
        release(&mut state);
        let body = state.fragments[0].assembly.body();
        let center = Vec2::new(30.0, 10.0);
        state.physics.set_pose(body, center, angle, true);
        state.physics.set_pose(
            state.spaceling.body(),
            center + Vec2::new(0.0, 2.0).rotate_radians(angle),
            angle,
            true,
        );
        state
            .physics
            .set_velocity(state.spaceling.body(), Vec2::ZERO, 0.0, true);
        state.physics.step(1.0 / FIXED_HZ as f32);
        state.select_tool(MiningTool::Precision);
        let before = state.fragments[0].terrain.clone();
        let snapshot = state.mining_snapshot();
        let target = snapshot.target.unwrap();
        assert_eq!(target.body, body.entity);
        assert_eq!(target.cell, CellCoord::new(2, 2));
        let predicted = before
            .brush_cells(snapshot.brush.unwrap())
            .unwrap()
            .map(|(c, _)| c)
            .collect::<Vec<_>>();
        state.mining.controls = MiningControls {
            held: true,
            aim: Vec2::new(0.0, -1.0).rotate_radians(angle),
            ..Default::default()
        };
        state.queue_mining(1.0 / FIXED_HZ as f32);
        state.commit_edits();
        let actual = before
            .cells()
            .iter()
            .zip(state.fragments[0].terrain.cells())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| CellCoord::new(i as i32 % 5, i as i32 / 5))
            .collect::<Vec<_>>();
        assert_eq!(actual, predicted);
        assert_eq!(state.recovered, MiningInventory::default());
        // Rebuilt colliders enter ray queries at the normal physics boundary.
        state.physics.step(1.0 / FIXED_HZ as f32);
        state.select_tool(MiningTool::Excavator);
        state.config.mining_tools[MiningTool::Excavator as usize].damage = 255;
        state.mining.cooldown = 0;
        state.queue_mining(1.0 / FIXED_HZ as f32);
        state.commit_edits();
        assert!(state.fragments.is_empty());
        assert!(!state.physics.contains_entity(body.entity));
        assert_eq!(
            state.recovered,
            MiningInventory {
                rock_cells: 6,
                ore_cells: 9
            }
        );
        step(&mut state, &[]);
        assert!(state.mining_snapshot().target.is_none());
        for _ in 0..30 {
            step(&mut state, &[]);
        }
        assert_eq!(
            state.recovered,
            MiningInventory {
                rock_cells: 6,
                ore_cells: 9
            }
        );
    }
}

#[test]
fn durability_only_edits_do_not_scan_connectivity_and_empty_planet_keeps_its_source() {
    let mut state = fixture(0.0);
    state.pending_edits.push(
        TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(9, 24),
                radius: 0,
            },
            mode: EditMode::Damage(1),
        }
        .into(),
    );
    state.commit_edits();
    assert!(state.fragments.is_empty());
    // The timer includes only the empty branch when there is no removal;
    // no terrain transfer or new physical body can occur.
    assert_eq!(state.last_edit.detached_cells, 0);
    state.pending_edits.push(
        TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(15, 15),
                radius: 100,
            },
            mode: EditMode::Remove,
        }
        .into(),
    );
    state.commit_edits();
    assert!(state.fragments.is_empty());
    assert_eq!(state.geometry.rectangle_count(), 0);
    assert!(state.physics.contains_entity(PLANET_ID));
    for _ in 0..3 {
        step(&mut state, &[]);
    }
    assert_eq!(state.recovered, MiningInventory::default());
}
