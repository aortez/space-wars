//! A repeatable capture workflow using real landing, transfer and takeoff in
//! the main match's claim/damage/lifecycle host. No ownership or hits are staged.
use engine_common::Scenario;
use engine_rapier::terrain::{GrainShape, LooseTerrainConfig};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        PlanetClaimStatus, SurfaceSortieAction, SurfaceSortieScenario, SurfaceSortieState,
    },
};
use serde_json::{Value, json};
use std::time::Duration;

const DT: Duration = Duration::from_nanos(16_666_667);

fn run(shape: GrainShape) -> Value {
    let mut state = SurfaceSortieScenario::init_material_combat_flight(42, &[]);
    state.enable_match_rules();
    state
        .enable_loose_terrain_with_config(LooseTerrainConfig {
            shape,
            ..Default::default()
        })
        .unwrap();
    let initial = state.terrain_diagnostics().occupied_cells;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut trace = Vec::new();
    let mut step = |state: &mut SurfaceSortieState, actions: [SurfaceSortieAction; 2]| {
        SurfaceSortieScenario::step(
            state,
            &[
                actions[0].encode(PlayerId::PLAYER_1),
                actions[1].encode(PlayerId::PLAYER_2),
            ],
            DT,
        );
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty());
        assert_eq!(audit.occupied_cells + audit.loose_cells as u64, initial);
        for byte in SurfaceSortieScenario::observe(state).payload {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
        let p1 = state.observation(0);
        if p1.tick.is_multiple_of(30) {
            trace.push(json!({"tick":p1.tick,"claim":p1.planet_claim,
                "forces":state.surface_force_observations(),"p2_landing":state.observation(1).landing,
                "round":state.match_observation()}));
        }
    };
    for _ in 0..240 {
        step(&mut state, Default::default());
    }
    step(
        &mut state,
        [
            SurfaceSortieAction {
                interact_held: true,
                ..Default::default()
            },
            SurfaceSortieAction::default(),
        ],
    );
    for _ in 0..240 {
        step(&mut state, Default::default());
    }
    let blocked = state.observation(0).planet_claim.unwrap();
    assert_eq!(blocked.status, PlanetClaimStatus::OpposingSurfaceForces);
    assert_eq!(blocked.opposition.ships, 1);
    assert_eq!(blocked.owner, None);
    assert!(blocked.flag.is_none());
    let paused = SurfaceSortieScenario::observe(&state).payload;
    SurfaceSortieScenario::step(&mut state, &[], Duration::ZERO);
    assert_eq!(paused, SurfaceSortieScenario::observe(&state).payload);
    let departure = [
        SurfaceSortieAction::default(),
        SurfaceSortieAction {
            primary_held: true,
            ..Default::default()
        },
    ];
    step(&mut state, departure);
    let cleared = state.observation(0).planet_claim.unwrap();
    assert!(cleared.opposition.is_empty());
    for _ in 0..240 {
        step(&mut state, departure);
    }
    let secured = state.observation(0).planet_claim.unwrap();
    assert_eq!(secured.owner, Some(PlayerId::PLAYER_1));
    assert_eq!(secured.captures, 1);
    assert!(state.match_outcome().is_none());
    json!({"seed":42,"shape":shape,"ticks":state.observation(0).tick,
        "blocked":blocked,"cleared":cleared,"secured":secured,"trace":trace,
        "observation_hash":format!("{hash:016x}"),"material":state.terrain_diagnostics()})
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.is_empty() || args == ["--replay"],
        "usage: surface_opposition [--replay]"
    );
    let replay = !args.is_empty();
    let cases: Vec<_> = [GrainShape::Round, GrainShape::Hexagon]
        .into_iter()
        .map(|shape| {
            let result = run(shape);
            if replay {
                assert_eq!(result, run(shape));
            }
            result
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"replay_checked":replay,"cases":cases})).unwrap()
    );
}
