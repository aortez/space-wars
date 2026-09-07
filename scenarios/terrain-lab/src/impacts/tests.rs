use super::*;
use crate::{
    FIXED_HZ, MiningInventory, ORE, ROCK, TerrainFragment, TerrainLabConfig, TerrainLabScenario,
};
use engine_common::Scenario;
use engine_rapier::{
    terrain::{TerrainAssembly, TerrainSpec},
    world::{BodyKind, BodySpec},
};
use engine_terrain::{CellCoord, Material, MaterialId, Terrain, TerrainGeometry};

const DT: f32 = 1.0 / FIXED_HZ as f32;

fn materials() -> Vec<Material> {
    vec![
        Material {
            id: ROCK,
            hardness: 100,
        },
        Material {
            id: ORE,
            hardness: 180,
        },
    ]
}

fn fixture(material: MaterialId) -> TerrainLabState {
    let mut state = TerrainLabScenario::init(
        TerrainLabConfig {
            radius: 7.5,
            gravity_acceleration: 0.0,
            angular_velocity: 0.0,
            orbit_radius: 0.0,
            ..Default::default()
        },
        42,
    );
    let terrain = Terrain::generate(31, 31, 0.5, materials(), |p| {
        if p.y <= 15 {
            material
        } else {
            MaterialId::VOID
        }
    })
    .unwrap();
    replace_planet(&mut state, terrain);
    state
        .physics
        .set_pose(state.spaceling.body(), Vec2::new(100.0, 50.0), 0.0, true);
    state.physics.step(DT);
    state
}

fn replace_planet(state: &mut TerrainLabState, terrain: Terrain) {
    state.physics.remove_entity(PLANET_ID);
    state.geometry = TerrainGeometry::new(&terrain);
    state.terrain_hash = terrain.hash();
    state.terrain = terrain;
    state.terrain_assembly = TerrainAssembly::insert(
        &mut state.physics,
        PLANET_ID,
        BodySpec {
            kind: BodyKind::KinematicVelocity,
            ..Default::default()
        },
        &state.terrain,
        &state.geometry,
        TerrainSpec::default(),
    )
    .unwrap();
}

fn rock(state: &mut TerrainLabState, side: u32, position: Vec2, velocity: Vec2) -> PhysicsId {
    let terrain = Terrain::generate(side, side, 0.5, materials(), |p| {
        if side > 1 && p.x == side as i32 / 2 {
            ORE
        } else {
            ROCK
        }
    })
    .unwrap();
    let geometry = TerrainGeometry::new(&terrain);
    let id = PhysicsId::new(state.next_fragment_id);
    state.next_fragment_id += 1;
    let assembly = TerrainAssembly::insert(
        &mut state.physics,
        id,
        BodySpec {
            position,
            linear_velocity: velocity,
            ccd_enabled: true,
            ..Default::default()
        },
        &terrain,
        &geometry,
        TerrainSpec::default(),
    )
    .unwrap();
    state.fragments.push(TerrainFragment {
        id,
        hash: terrain.hash(),
        terrain,
        geometry,
        assembly,
        edited_chunks: Vec::new(),
    });
    id
}

fn occupied(state: &TerrainLabState) -> usize {
    state
        .terrain_bodies()
        .map(|(t, _, _, _)| {
            t.cells()
                .iter()
                .filter(|c| c.material != MaterialId::VOID)
                .count()
        })
        .sum()
}

fn step(state: &mut TerrainLabState) {
    TerrainLabScenario::step(state, &[], std::time::Duration::from_secs_f32(DT));
}

fn physical_step(state: &mut TerrainLabState, gravity: Vec2) {
    state.commit_edits();
    let before = state.capture_impact_motions();
    for fragment in &state.fragments {
        state
            .physics
            .apply_velocity_delta(fragment.assembly.body(), gravity * DT, true);
    }
    state.physics.step(DT);
    state.queue_impact_damage(&before);
    state.tick += 1;
}

#[test]
fn hard_impact_queues_local_damage_to_both_bodies_and_resumes_before_commit() {
    let mut state = fixture(ROCK);
    rock(&mut state, 5, Vec2::new(0.0, 4.0), Vec2::new(0.0, -16.0));
    let terrain_before = state.terrain.clone();
    let fragment_before = state.fragments[0].terrain.clone();
    let initial_cells = occupied(&state);
    for _ in 0..60 {
        step(&mut state);
        if state.impact_stats().hits > 0 {
            break;
        }
    }
    assert_eq!(state.impact_stats().hits, 1, "{:?}", state.impact_stats());
    assert_eq!(
        state.terrain, terrain_before,
        "contacts queue damage for the next tick"
    );
    assert_eq!(state.fragments[0].terrain, fragment_before);
    assert_eq!(
        state.pending_edits.len(),
        2,
        "one hit per body pair, not per rectangle"
    );
    let mut restored = state.clone();
    for _ in 0..120 {
        step(&mut state);
        step(&mut restored);
        assert_eq!(
            TerrainLabScenario::observe(&state).payload,
            TerrainLabScenario::observe(&restored).payload
        );
    }
    assert_ne!(state.terrain, terrain_before);
    assert_ne!(state.fragments[0].terrain, fragment_before);
    assert!(state.impact_stats().destroyed_cells > 0);
    assert_eq!(state.recovered, MiningInventory::default());
    assert_eq!(
        occupied(&state) + state.removed_cells as usize,
        initial_cells
    );
}

#[test]
fn rock_breaks_before_ore_and_a_destroyed_fragment_leaves_no_body() {
    for material in [ROCK, ORE] {
        let mut state = fixture(material);
        state.config.impacts.max_damage = 120;
        state.config.impacts.max_radius = 0.0;
        let id = rock(&mut state, 1, Vec2::new(0.0, 2.0), Vec2::new(0.0, -40.0));
        for _ in 0..30 {
            step(&mut state);
            if state.impact_stats().hits > 0 {
                break;
            }
        }
        assert_eq!(state.impact_stats().hits, 1, "{:?}", state.impact_stats());
        let edit = state
            .pending_edits
            .iter()
            .find(|edit| edit.body == PLANET_ID)
            .unwrap()
            .edit;
        let Brush::Circle { center, .. } = edit.brush else {
            panic!("impact circle");
        };
        step(&mut state);
        let cell = state.terrain.cell(center).unwrap();
        assert_eq!(
            cell.material,
            if material == ROCK {
                MaterialId::VOID
            } else {
                ORE
            }
        );
        assert_eq!(cell.durability, if material == ROCK { 0 } else { 60 });
        assert!(!state.physics.contains_entity(id));
        assert!(state.fragments.is_empty());
        assert_eq!(state.recovered, MiningInventory::default());
    }
}

#[test]
fn gentle_landing_resting_weight_and_remeshing_do_not_erode_terrain() {
    let mut state = fixture(ROCK);
    rock(&mut state, 5, Vec2::new(0.0, 1.6), Vec2::new(0.0, -2.0));
    for _ in 0..120 {
        physical_step(&mut state, Vec2::new(0.0, -18.0));
    }
    assert_eq!(state.impact_stats().hits, 0, "gentle landing");
    for _ in 0..180 {
        physical_step(&mut state, Vec2::new(0.0, -400.0));
    }
    assert_eq!(
        state.impact_stats().hits,
        0,
        "resting gravity is not impact work"
    );
    state.pending_edits.push(
        TerrainEdit {
            brush: Brush::Circle {
                center: CellCoord::new(1, 1),
                radius: 0,
            },
            mode: EditMode::Damage(1),
        }
        .into(),
    );
    state.commit_edits();
    let hash = state.terrain_hash();
    for _ in 0..180 {
        physical_step(&mut state, Vec2::new(0.0, -400.0));
    }
    assert_eq!(state.impact_stats().hits, 0);
    assert_eq!(state.terrain_hash(), hash);
    assert_eq!(state.removed_cells, 0);
}

#[test]
fn ccd_impact_on_translating_rotated_terrain_keeps_the_local_hit_cells() {
    let mut state = fixture(ROCK);
    let id = rock(&mut state, 1, Vec2::new(0.0, 3.0), Vec2::new(0.0, -300.0));
    let angle = 0.73;
    let translation = Vec2::new(80.0, -50.0);
    let common_velocity = Vec2::new(100.0, 30.0);
    for body in [
        state.terrain_assembly.body(),
        BodyId::new(id, BodyRole::PRIMARY),
    ] {
        let before = state.physics.motion(body).unwrap();
        state.physics.set_pose(
            body,
            translation + before.position.rotate_radians(angle),
            angle,
            true,
        );
        state.physics.set_velocity(
            body,
            common_velocity + before.linear_velocity.rotate_radians(angle),
            0.0,
            true,
        );
    }
    for _ in 0..5 {
        physical_step(&mut state, Vec2::ZERO);
        if state.impact_stats().hits > 0 {
            break;
        }
    }
    assert_eq!(state.impact_stats().hits, 1, "{:?}", state.impact_stats());
    assert_eq!(state.pending_edits.len(), 2);
    let edit = state
        .pending_edits
        .iter()
        .find(|edit| edit.body == PLANET_ID)
        .unwrap()
        .edit;
    let Brush::Circle { center, radius } = edit.brush else {
        panic!("impact circle");
    };
    let planet = state.planet_motion();
    let fragment = state
        .physics
        .motion(BodyId::new(id, BodyRole::PRIMARY))
        .unwrap();
    let local = (fragment.position - planet.position).rotate_radians(-planet.angle);
    // CCD may clamp part of the common translation. The scar must still be
    // underneath the actual fragment, not at an old world-space impact point.
    assert!(
        (state.terrain.cell_center(center).x - local.x).abs() <= 0.51 && center.y == 15,
        "{center:?}, fragment in planet frame: {local:?}"
    );
    assert!(radius <= 4);
    physical_step(&mut state, Vec2::ZERO);
    assert_eq!(
        state.terrain.cell(center).unwrap().material,
        MaterialId::VOID
    );
    assert!(state.impact_stats().destroyed_cells > 0);
    assert_eq!(state.recovered, MiningInventory::default());
}

#[test]
fn simultaneous_impacts_have_a_stable_budget_and_no_delayed_backlog() {
    let mut state = fixture(ROCK);
    state.config.impacts.max_hits_per_tick = 2;
    for x in [-4.0, -2.0, 0.0, 2.0, 4.0] {
        rock(&mut state, 1, Vec2::new(x, 1.0), Vec2::new(0.0, -40.0));
    }
    for _ in 0..10 {
        physical_step(&mut state, Vec2::ZERO);
        if state.impact_stats().hits > 0 {
            break;
        }
    }
    assert_eq!(state.impact_stats().hits, 2);
    assert_eq!(state.impact_stats().budget_dropped, 3);
    let damaged = state
        .pending_edits
        .iter()
        .filter(|e| e.body != PLANET_ID)
        .map(|e| e.body.value())
        .collect::<Vec<_>>();
    assert_eq!(damaged, vec![3, 4], "equal impacts use stable body order");
    assert!(state.pending_edits.len() <= 4);
    for _ in 0..120 {
        physical_step(&mut state, Vec2::ZERO);
    }
    assert_eq!(state.impact_stats().hits, 2);
    assert!(state.pending_edits.is_empty());
    assert_eq!(state.recovered, MiningInventory::default());
}

#[test]
fn impact_can_sever_an_overhang_and_release_a_second_damaging_fall() {
    let mut state = fixture(ROCK);
    let terrain = Terrain::generate(31, 31, 0.5, materials(), |p| {
        if p.y <= 10 || (p.x == 5 && p.y <= 22) || ((5..=25).contains(&p.x) && p.y == 22) {
            ROCK
        } else {
            MaterialId::VOID
        }
    })
    .unwrap();
    replace_planet(&mut state, terrain);
    rock(&mut state, 1, Vec2::new(-1.5, 5.0), Vec2::new(0.0, -60.0));
    let initial = occupied(&state);
    for _ in 0..5 {
        physical_step(&mut state, Vec2::new(0.0, -18.0));
        if state.impact_stats().hits > 0 {
            break;
        }
    }
    assert_eq!(state.impact_stats().hits, 1);
    physical_step(&mut state, Vec2::new(0.0, -18.0));
    assert!(
        state.fragments.iter().any(|f| f.id.value() > 3),
        "the overhang must detach"
    );
    for _ in 0..240 {
        physical_step(&mut state, Vec2::new(0.0, -18.0));
    }
    assert!(state.impact_stats().hits >= 2, "{:?}", state.impact_stats());
    assert!(state.impact_stats().destroyed_cells > 2);
    assert_eq!(occupied(&state) + state.removed_cells as usize, initial);
    assert_eq!(state.recovered, MiningInventory::default());
}

#[test]
fn fragments_damage_each_other_using_relative_speed_and_both_masses() {
    let mut state = fixture(ROCK);
    for direction in [-1.0, 1.0] {
        rock(
            &mut state,
            3,
            Vec2::new(direction * 3.0, 30.0),
            Vec2::new(-direction * 16.0, 0.0),
        );
    }
    let planet = state.terrain.clone();
    let initial = occupied(&state);
    for _ in 0..30 {
        physical_step(&mut state, Vec2::ZERO);
        if state.impact_stats().hits > 0 {
            break;
        }
    }
    assert_eq!(state.impact_stats().hits, 1);
    assert_eq!(
        state
            .pending_edits
            .iter()
            .map(|e| e.body.value())
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    assert!((state.impact_stats().last_speed - 32.0).abs() < 0.1);
    physical_step(&mut state, Vec2::ZERO);
    assert_eq!(state.terrain, planet);
    assert!(state.impact_stats().destroyed_cells > 0);
    assert_eq!(occupied(&state) + state.removed_cells as usize, initial);
    assert_eq!(state.recovered, MiningInventory::default());
}

#[test]
fn common_motion_and_spaceling_impacts_do_not_damage_terrain() {
    let mut state = fixture(ROCK);
    let id = rock(&mut state, 5, Vec2::new(0.0, 1.6), Vec2::new(0.0, -2.0));
    let angle = -0.61;
    let common = Vec2::new(80.0, 20.0);
    for body in [
        state.terrain_assembly.body(),
        BodyId::new(id, BodyRole::PRIMARY),
    ] {
        let motion = state.physics.motion(body).unwrap();
        state
            .physics
            .set_pose(body, motion.position.rotate_radians(angle), angle, true);
        state.physics.set_velocity(
            body,
            common + motion.linear_velocity.rotate_radians(angle),
            0.0,
            true,
        );
    }
    let initial = state.terrain_hash();
    for _ in 0..120 {
        physical_step(&mut state, Vec2::ZERO);
    }
    assert_eq!(state.impact_stats().hits, 0);
    assert_eq!(state.terrain_hash(), initial);

    let mut state = fixture(ROCK);
    rock(&mut state, 1, Vec2::new(50.0, 50.0), Vec2::ZERO);
    state
        .physics
        .set_pose(state.spaceling.body(), Vec2::new(0.0, 3.0), 0.0, true);
    state
        .physics
        .set_velocity(state.spaceling.body(), Vec2::new(0.0, -50.0), 0.0, true);
    let initial = state.terrain_hash();
    for _ in 0..60 {
        physical_step(&mut state, Vec2::ZERO);
    }
    assert_eq!(state.impact_stats().hits, 0);
    assert_eq!(state.terrain_hash(), initial);
}

#[test]
fn repeated_pressure_is_one_hit_but_separation_rearms_the_pair() {
    let mut state = fixture(ROCK);
    state.config.impacts.max_damage = 1;
    state.config.impacts.max_radius = 0.0;
    let id = rock(&mut state, 5, Vec2::new(0.0, 3.0), Vec2::new(0.0, -16.0));
    let body = BodyId::new(id, BodyRole::PRIMARY);
    for _ in 0..30 {
        physical_step(&mut state, Vec2::ZERO);
        if state.impact_stats().hits > 0 {
            break;
        }
    }
    assert_eq!(state.impact_stats().hits, 1);
    for _ in 0..30 {
        state
            .physics
            .set_velocity(body, Vec2::new(0.0, -16.0), 0.0, true);
        physical_step(&mut state, Vec2::ZERO);
    }
    assert_eq!(
        state.impact_stats().hits,
        1,
        "sustained pressure is not repeated impact damage"
    );
    state.physics.set_pose(body, Vec2::new(0.0, 4.0), 0.0, true);
    state.physics.set_velocity(body, Vec2::ZERO, 0.0, true);
    for _ in 0..state.config.impacts.rearm_ticks + 2 {
        physical_step(&mut state, Vec2::ZERO);
    }
    state
        .physics
        .set_velocity(body, Vec2::new(0.0, -16.0), 0.0, true);
    for _ in 0..30 {
        physical_step(&mut state, Vec2::ZERO);
        if state.impact_stats().hits > 1 {
            break;
        }
    }
    assert_eq!(state.impact_stats().hits, 2);
}

#[test]
fn impact_tuning_is_bounded_and_pausing_retains_a_queued_hit() {
    let state = TerrainLabScenario::init(
        TerrainLabConfig {
            impacts: TerrainImpactConfig {
                min_speed: f32::NAN,
                damage_per_energy: f32::INFINITY,
                max_radius: 10000.0,
                max_damage: 0,
                max_hits_per_tick: usize::MAX,
                rearm_ticks: 0,
                enabled: true,
            },
            ..Default::default()
        },
        42,
    );
    assert_eq!(state.config.impacts.min_speed, 6.0);
    assert_eq!(state.config.impacts.damage_per_energy, 1.0);
    assert_eq!(state.config.impacts.max_radius, 2.0);
    assert_eq!(state.config.impacts.max_hits_per_tick, 8);
    assert_eq!(state.config.impacts.max_damage, 1);
    assert_eq!(state.config.impacts.rearm_ticks, 1);
    let mut state = fixture(ROCK);
    rock(&mut state, 1, Vec2::new(0.0, 1.0), Vec2::new(0.0, -40.0));
    for _ in 0..10 {
        step(&mut state);
        if state.impact_stats().hits > 0 {
            break;
        }
    }
    assert!(!state.pending_edits.is_empty());
    let before = TerrainLabScenario::observe(&state).payload;
    TerrainLabScenario::step(&mut state, &[], std::time::Duration::ZERO);
    assert_eq!(TerrainLabScenario::observe(&state).payload, before);
    step(&mut state);
    assert!(state.impact_stats().destroyed_cells > 0);
}
