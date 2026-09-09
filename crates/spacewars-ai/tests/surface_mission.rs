use engine_common::{CombatBreakSettings, Scenario};
use scenario_spacewars::{
    PlayerId, ShipForm,
    surface_sortie::{
        SurfaceSortieScenario,
        pilot::{LANDING_SITE_COUNT, LandingSiteId},
    },
};
use spacewars_ai::{
    BrainReset,
    combat_pilot::CombatIntent,
    mission_pilot::{MaterialMissionPilot, MissionGoal},
};
use std::{collections::BTreeSet, time::Duration};
const DT: Duration = Duration::from_nanos(16_666_667);
fn context(seat: usize) -> BrainReset {
    BrainReset {
        actor: PlayerId::from_index(seat).unwrap(),
        episode_seed: 42,
    }
}

#[test]
fn world_observation_is_read_only_and_destination_never_changes_physical_support() {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let before = state.observation(0);
    let request = Some(LandingSiteId {
        planet: 1,
        bearing: LANDING_SITE_COUNT,
    });
    let a = state.mission_observation(0, request);
    let b = state.mission_observation(0, request);
    assert_eq!(a, b);
    assert_eq!(state.observation(0), before);
    assert_eq!(a.planets.len(), 2);
    assert_eq!(a.local.combat.recovery.flight.pilot.planet.index, 0);
    assert!(a.local.combat.recovery.flight.pilot.sites.is_empty());
    assert_eq!(
        state.terrain_diagnostics().occupied_cells,
        2 * SurfaceSortieScenario::init_material(42, 1)
            .terrain_diagnostics()
            .occupied_cells
    );
}

#[test]
fn mission_identity_replay_clone_reset_and_ownership_replanning() {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut brain = MaterialMissionPilot::new(context(0), CombatBreakSettings::default());
    let mut o = state.mission_observation(0, brain.site_request());
    let first = brain.intent(&o);
    assert_eq!(brain.telemetry().target, Some(1));
    let before = brain.telemetry().clone();
    assert_eq!(brain.intent(&o), first);
    assert_eq!(brain.telemetry(), &before);
    let mut copy = brain.clone();
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.planets[1].claim.as_mut().unwrap().owner = Some(PlayerId::PLAYER_1);
    assert_eq!(brain.intent(&o), copy.intent(&o));
    assert_eq!(brain.telemetry(), copy.telemetry());
    assert_eq!(brain.telemetry().target, Some(0));
    assert_eq!(brain.telemetry().replans, 1);
    for fault in 0..8 {
        let mut bad = o.clone();
        match fault {
            0 => bad.version = 99,
            1 => bad.local.version = 99,
            2 => bad.local.combat.version = 99,
            3 => bad.local.combat.recovery.version = 99,
            4 => bad.local.combat.recovery.flight.version = 99,
            5 => bad.local.combat.recovery.flight.flight.version = 99,
            6 => bad.local.combat.recovery.flight.pilot.version = 99,
            _ => bad.local.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_2,
        }
        let before = brain.telemetry().clone();
        assert_eq!(brain.intent(&bad), CombatIntent::default());
        assert_eq!(brain.telemetry(), &before);
    }
    brain.reset(context(0));
    assert_eq!(
        brain.telemetry(),
        MaterialMissionPilot::new(context(0), CombatBreakSettings::default()).telemetry()
    );
}

#[test]
fn loss_during_transfer_enters_recovery_and_transfer_timeout_defers_destination() {
    let mut state = SurfaceSortieScenario::init_material_travel(42, false);
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut brain = MaterialMissionPilot::new(context(0), CombatBreakSettings::default());
    let mut o = state.mission_observation(0, brain.site_request());
    brain.intent(&o);
    o.local.combat.recovery.flight.pilot.tick += 61 * 60;
    brain.intent(&o);
    assert_eq!(brain.telemetry().target, None);
    assert_eq!(brain.telemetry().replans, 1);
    o.local.combat.recovery.flight.pilot.tick += 1;
    brain.intent(&o);
    assert_ne!(brain.telemetry().target, Some(1));
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.local.combat.recovery.flight.pilot.ship_form = ShipForm::EscapePod;
    o.local
        .combat
        .recovery
        .flight
        .pilot
        .recovery
        .as_mut()
        .unwrap()
        .ships_lost += 1;
    brain.intent(&o);
    assert!(brain.telemetry().recovery.is_some());
    assert!(matches!(
        brain.telemetry().goal,
        MissionGoal::Recover | MissionGoal::Blocked
    ));
    assert_eq!(brain.telemetry().target, None);
}

#[test]
fn both_seats_physically_capture_board_and_depart_from_both_planets() {
    for (seat, mirror, bearing) in [(0, false), (1, false), (0, true), (1, true)]
        .into_iter()
        .flat_map(|(seat, mirror)| {
            [
                -std::f32::consts::FRAC_PI_2,
                0.0,
                std::f32::consts::FRAC_PI_2,
            ]
            .map(|bearing| (seat, mirror, bearing))
        })
    {
        let mut state = SurfaceSortieScenario::init_material_travel_trial(42, mirror, bearing);
        let mut brain = MaterialMissionPilot::new(context(seat), CombatBreakSettings::default());
        let initial = state.terrain_diagnostics().occupied_cells;
        // Match the three-minute acceptance window: a local landing can fail,
        // defer for thirty seconds and still complete on the next attempt.
        for _ in 0..180 * 60 {
            let o = state.mission_observation(seat, brain.site_request());
            let mut intent = brain.intent(&o);
            intent.weapons = Default::default();
            let actions = intent.encode(context(seat).actor);
            SurfaceSortieScenario::step(&mut state, &actions, DT);
            if brain.telemetry().completed_sorties >= 2 {
                break;
            }
        }
        let destinations: BTreeSet<_> = brain
            .telemetry()
            .events
            .iter()
            .filter(|e| e.kind == "departed")
            .filter_map(|e| e.planet)
            .collect();
        assert_eq!(
            destinations,
            BTreeSet::from([0, 1]),
            "seat {seat} mirror {mirror}: {:?}",
            brain.telemetry()
        );
        let o = state.observation(seat);
        assert!(o.transfers >= 4);
        assert!(
            o.planet_claims
                .iter()
                .all(|claim| claim.owner == Some(context(seat).actor))
        );
        let audit = state.terrain_diagnostics();
        assert!(audit.issues.is_empty());
        assert_eq!(audit.occupied_cells + audit.removed_cells, initial);
    }
}
