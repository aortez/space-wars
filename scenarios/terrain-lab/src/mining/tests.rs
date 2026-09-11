use super::*;
use crate::{PLANET_ID, TerrainLabAction, TerrainLabConfig, TerrainLabScenario};
use engine_common::{Action, PointerAction, PointerPhase, RenderPoint, Scenario};
use engine_rapier::{
    terrain::{TerrainAssembly, TerrainSpec},
    world::{BodyKind, BodySpec},
};
use engine_terrain::{Material, Terrain, TerrainGeometry};
use std::time::Duration;

mod tools;

fn step(state: &mut TerrainLabState, actions: &[Action]) {
    TerrainLabScenario::step(
        state,
        actions,
        Duration::from_secs_f64(1.0 / f64::from(FIXED_HZ)),
    );
}

fn drill(held: bool) -> Action {
    TerrainLabAction::Mining(MiningControls {
        held,
        ..Default::default()
    })
    .encode()
}

fn pointer(position: Vec2, phase: PointerPhase) -> Action {
    Action::Pointer(PointerAction {
        position: RenderPoint::new(position.x, position.y),
        phase,
    })
}

/// A thin surface and a second layer beyond the beam's reach. The character
/// floats above the first layer so hardness tests do not depend on falling.
/// Side walls connect both layers outside the cut, keeping this a mining fixture.
fn fixture(material: MaterialId) -> TerrainLabState {
    let mut state = TerrainLabScenario::init(
        TerrainLabConfig {
            radius: 5.0,
            angular_velocity: 0.0,
            orbit_radius: 0.0,
            gravity_acceleration: 0.0,
            ..Default::default()
        },
        42,
    );
    state.physics.remove_entity(PLANET_ID);
    state.terrain = Terrain::generate(
        25,
        25,
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
        |cell| {
            if cell.y == 12
                || cell.y == 2
                || ((cell.x == 0 || cell.x == 24) && (2..=12).contains(&cell.y))
            {
                material
            } else {
                MaterialId::VOID
            }
        },
    )
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
        .set_pose(state.spaceling.body(), Vec2::new(0.0, 2.0), 0.0, true);
    state.physics.step(1.0 / FIXED_HZ as f32);
    state
}

#[test]
fn repeated_drilling_respects_hardness_and_only_credits_removed_material_once() {
    for (material, pulses) in [(ROCK, 5), (ORE, 9)] {
        let mut state = fixture(material);
        let before = state.terrain.clone();
        let target = state.mining_snapshot().target.unwrap();
        assert_eq!(target.cell, CellCoord::new(12, 12));
        step(&mut state, &[drill(true)]);
        assert_eq!(
            state.terrain.cell(target.cell).unwrap().durability,
            target.hardness - DRILL_DAMAGE
        );
        assert_eq!(state.recovered, MiningInventory::default());
        // The final pulse occurs at tick 25 for rock and tick 49 for ore.
        for _ in 1..(pulses - 1) * DRILL_INTERVAL_TICKS {
            step(&mut state, &[]);
        }
        assert_eq!(
            state.terrain.cell(target.cell).unwrap().durability,
            DRILL_DAMAGE
        );
        assert_eq!(state.recovered.cells(material), 0);
        step(&mut state, &[]);
        assert_eq!(
            state.terrain.cell(target.cell).unwrap().material,
            MaterialId::VOID
        );
        let removed = before
            .cells()
            .iter()
            .zip(state.terrain.cells())
            .filter(|(before, after)| {
                before.material == material && after.material == MaterialId::VOID
            })
            .count() as u64;
        assert_eq!(removed, 5);
        assert_eq!(state.recovered.cells(material), removed);
        assert_eq!(state.recovered_area(material), 1.25);
        assert_eq!(
            state
                .recovered
                .cells(if material == ROCK { ORE } else { ROCK }),
            0
        );
        assert!(
            state.mining_snapshot().target.is_none(),
            "the next layer is beyond reach"
        );
        let hash = state.terrain_hash();
        for _ in 0..120 {
            step(&mut state, &[]);
        }
        assert_eq!(state.recovered.cells(material), removed);
        assert_eq!(state.terrain_hash(), hash);
        assert_eq!(
            state.terrain.cell(CellCoord::new(12, 2)),
            before.cell(CellCoord::new(12, 2))
        );
    }
}

#[test]
fn releasing_and_tapping_cannot_bypass_the_drill_cooldown() {
    let mut held = fixture(ORE);
    let mut tapped = held.clone();
    for tick in 0..25 {
        step(&mut held, &[drill(true)]);
        step(&mut tapped, &[drill(tick % 2 == 0)]);
        assert_eq!(held.terrain(), tapped.terrain());
    }
    step(&mut tapped, &[drill(false)]);
    let hash = tapped.terrain_hash();
    for _ in 0..30 {
        step(&mut tapped, &[]);
    }
    assert_eq!(tapped.terrain_hash(), hash, "release must stop damage");
}

#[test]
fn pointer_mines_the_nearest_surface_and_cancel_stops_a_stationary_hold() {
    let mut state = fixture(ROCK);
    let deep_point = Vec2::new(0.0, -50.0);
    step(&mut state, &[pointer(deep_point, PointerPhase::Press)]);
    assert_eq!(
        state
            .terrain
            .cell(CellCoord::new(12, 12))
            .unwrap()
            .durability,
        80
    );
    assert_eq!(
        state
            .terrain
            .cell(CellCoord::new(12, 2))
            .unwrap()
            .durability,
        100
    );
    for _ in 0..6 {
        step(&mut state, &[]);
    }
    assert_eq!(
        state
            .terrain
            .cell(CellCoord::new(12, 12))
            .unwrap()
            .durability,
        60
    );
    let observation = TerrainLabScenario::observe(&state).payload;
    let cooldown = state.mining.cooldown;
    TerrainLabScenario::step(
        &mut state,
        &[pointer(deep_point, PointerPhase::Cancel)],
        Duration::ZERO,
    );
    assert_eq!(TerrainLabScenario::observe(&state).payload, observation);
    assert_eq!(state.mining.cooldown, cooldown);
    let hash = state.terrain_hash();
    for _ in 0..20 {
        step(&mut state, &[pointer(deep_point, PointerPhase::Drag)]);
    }
    assert_eq!(
        state.terrain_hash(),
        hash,
        "orphan drags must not restart a cancelled drill"
    );
    assert!(!state.mining_snapshot().active);
    state
        .physics
        .set_pose(state.spaceling.body(), Vec2::new(0.0, 6.0), 0.0, true);
    step(&mut state, &[pointer(deep_point, PointerPhase::Press)]);
    for _ in 0..60 {
        step(&mut state, &[]);
    }
    assert_eq!(
        state.terrain_hash(),
        hash,
        "a distant pointer cannot extend tool reach"
    );
    step(&mut state, &[pointer(deep_point, PointerPhase::Release)]);
    assert!(!state.mining_snapshot().active);
}

#[test]
fn world_aim_quantizes_hits_on_a_translated_rotated_planet_before_it_moves() {
    let mut state = fixture(ORE);
    let translation = Vec2::new(7.0, -3.0);
    let angle = 0.7;
    state
        .physics
        .set_pose(state.terrain_assembly.body(), translation, angle, true);
    state.physics.set_pose(
        state.spaceling.body(),
        translation + Vec2::new(0.0, 2.0).rotate_radians(angle),
        angle,
        true,
    );
    state.physics.step(1.0 / FIXED_HZ as f32);
    let point = state.local_to_world(state.terrain.cell_center(CellCoord::new(12, 12)));
    step(&mut state, &[pointer(point, PointerPhase::Press)]);
    assert_eq!(
        state
            .terrain
            .cell(CellCoord::new(12, 12))
            .unwrap()
            .durability,
        160
    );
    assert_eq!(
        state
            .terrain
            .cell(CellCoord::new(12, 2))
            .unwrap()
            .durability,
        180
    );
}

#[test]
fn debug_cuts_and_external_damage_do_not_credit_the_mining_inventory() {
    let mut state = fixture(ROCK);
    let edit = TerrainLabAction::Edit(TerrainEdit {
        brush: Brush::Circle {
            center: CellCoord::new(12, 12),
            radius: 2,
        },
        mode: EditMode::Damage(255),
    })
    .encode();
    // The drill samples the same pre-edit world, but cannot collect the cells
    // already removed by an earlier debug edit in this commit.
    step(&mut state, &[edit, drill(true)]);
    assert_eq!(state.removed_cells, 5);
    assert_eq!(state.recovered, MiningInventory::default());
    step(
        &mut state,
        &[TerrainLabAction::controls(0.0, false, true, true, true)],
    );
    assert!(state.removed_cells > 5);
    assert_eq!(state.recovered, MiningInventory::default());
}

#[test]
fn mining_replays_and_resumes_mid_damage_while_the_character_falls() {
    let mut first = TerrainLabScenario::init(TerrainLabConfig::default(), 42);
    for _ in 0..13 {
        step(&mut first, &[drill(true)]);
    }
    let mut restored = first.clone();
    restored.terrain = bincode::deserialize(&bincode::serialize(first.terrain()).unwrap()).unwrap();
    restored.geometry = TerrainGeometry::new(restored.terrain());
    let start = first.spaceling_snapshot().motion.position;
    for tick in 0..300 {
        let action = TerrainLabAction::Mining(MiningControls {
            held: tick < 220,
            turn: if (80..100).contains(&tick) { 0.15 } else { 0.0 },
            ..Default::default()
        })
        .encode();
        step(&mut first, std::slice::from_ref(&action));
        step(&mut restored, &[action]);
        assert_eq!(
            TerrainLabScenario::observe(&first).payload,
            TerrainLabScenario::observe(&restored).payload
        );
        assert_eq!(first.mining_snapshot(), restored.mining_snapshot());
    }
    assert!(first.recovered.rock_cells + first.recovered.ore_cells > 0);
    assert!(
        first
            .spaceling_snapshot()
            .motion
            .position
            .distance_to(start)
            > 2.0
    );
    assert_eq!(first.terrain(), restored.terrain());
    assert_eq!(first.mining.cooldown, restored.mining.cooldown);
}

#[test]
fn mining_removes_support_and_the_character_lands_on_the_excavated_floor() {
    let mut state = TerrainLabScenario::init(
        TerrainLabConfig {
            angular_velocity: 0.0,
            orbit_radius: 0.0,
            ..Default::default()
        },
        42,
    );
    for _ in 0..120 {
        step(&mut state, &[]);
    }
    let before = state.spaceling_snapshot();
    assert!(before.grounded());
    for _ in 0..60 {
        step(&mut state, &[drill(true)]);
        if !state.spaceling_snapshot().grounded() {
            break;
        }
    }
    assert!(!state.spaceling_snapshot().grounded());
    assert!(state.recovered.rock_cells + state.recovered.ore_cells > 0);
    step(&mut state, &[drill(false)]);
    for _ in 0..180 {
        step(&mut state, &[]);
    }
    let after = state.spaceling_snapshot();
    assert!(
        after.grounded(),
        "the mined floor should provide normal contact support: {after:?}; before {before:?}; recovered {:?}",
        state.recovered
    );
    assert!(after.motion.position.y < before.motion.position.y - 0.4);
}

#[test]
fn aim_controls_are_validated_and_change_the_beam_without_mining() {
    let mut state = fixture(ROCK);
    let hash = state.terrain_hash();
    let action = TerrainLabAction::Mining(MiningControls {
        aim: Vec2::X,
        ..Default::default()
    })
    .encode();
    assert_eq!(TerrainLabAction::decode(&action).unwrap().encode(), action);
    step(&mut state, &[action]);
    let beam = state.mining_snapshot();
    assert!((beam.end - beam.origin).normalized().x > 0.99);
    assert_eq!(state.terrain_hash(), hash);
    step(
        &mut state,
        &[TerrainLabAction::Mining(MiningControls {
            turn: -1.0,
            ..Default::default()
        })
        .encode()],
    );
    assert!(state.mining_snapshot().end.y < beam.end.y);
    for invalid in [
        Action::scenario(3, vec![]),
        Action::scenario(3, vec![2; 14]),
        Action::scenario(3, [vec![1, 2], vec![0; 12]].concat()),
        TerrainLabAction::Mining(MiningControls {
            turn: f32::NAN,
            held: true,
            ..Default::default()
        })
        .encode(),
        TerrainLabAction::Mining(MiningControls {
            aim: Vec2::new(f32::INFINITY, 0.0),
            ..Default::default()
        })
        .encode(),
    ] {
        assert!(TerrainLabAction::decode(&invalid).is_none());
    }
    step(
        &mut state,
        &[pointer(Vec2::new(f32::NAN, 0.0), PointerPhase::Press)],
    );
    assert!(!state.mining_snapshot().active);
}
