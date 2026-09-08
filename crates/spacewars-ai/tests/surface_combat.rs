use engine_common::Scenario;
use scenario_spacewars::{PlayerId, surface_sortie::SurfaceSortieScenario};
use spacewars_ai::{BrainReset, combat_pilot::RulePilotV4};
use std::time::Duration;

#[test]
fn physical_duel_loses_rebuilds_and_fires_again_within_three_minutes() {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    let mut brains = [0, 1].map(|seat| {
        RulePilotV4::new(BrainReset {
            actor: PlayerId::from_index(seat).unwrap(),
            episode_seed: 42,
        })
    });
    for tick in 0..180 * 60 {
        let mut actions = Vec::new();
        for (seat, brain) in brains.iter_mut().enumerate() {
            let o = state.combat_observation(seat, brain.site_request());
            actions.extend(brain.intent(&o).encode(PlayerId::from_index(seat).unwrap()));
        }
        SurfaceSortieScenario::step(&mut state, &actions, Duration::from_nanos(16_666_667));
        if tick % 60 == 0 {
            assert!(state.terrain_diagnostics().issues.is_empty());
        }
        if brains.iter().any(|b| b.telemetry().combat_returns > 0) {
            break;
        }
    }
    let seat = brains
        .iter()
        .position(|b| b.telemetry().combat_returns > 0)
        .unwrap_or_else(|| {
            panic!(
                "no physical combat return: {:?}",
                brains.each_ref().map(|b| b.telemetry())
            )
        });
    let recovery = state.observation(seat).recovery.unwrap();
    assert!(recovery.ships_lost > 0 && recovery.pod_ejections > 0 && recovery.rebuilds > 0);
    assert!(state.combat_telemetry(1 - seat).cannon_hits > 0);
    assert!(state.combat_telemetry(seat).shells_fired > 0);
    assert_eq!(brains[seat].telemetry().completed_recoveries, 1);
    let supply = state.combat_observation(seat, None).supply.unwrap();
    assert!((0.0..=100.0).contains(&supply.energy_percent));
    assert!(
        supply.rounds_loaded <= 2,
        "rebuilt ship must use the same supply"
    );
}
