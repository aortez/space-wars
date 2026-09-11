use engine_common::Scenario;
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{LandingPhase, PilotLocation, SurfaceSortieAction, SurfaceSortieScenario},
};
use spacewars_ai::{
    BrainReset,
    pilot::{PilotBrain, PilotGoal, RulePilotV1},
};
use std::time::Duration;
#[path = "support/surface_cases.rs"]
mod cases;
const DT: Duration = Duration::from_nanos(16_666_667);

fn sortie(case: usize) {
    for seat in 0..2 {
        let owner = PlayerId::from_index(seat).unwrap();
        let seed = [7, 42][seat];
        let mut state =
            SurfaceSortieScenario::init_material_flight(seed, 2, &[(owner, cases::STARTS[case])]);
        let mut brain = RulePilotV1::new(BrainReset {
            actor: owner,
            episode_seed: seed,
        });
        let mut saw_landing = false;
        let mut saw_claim = false;
        let mut completion = None;
        for tick in 0..120 * 60 {
            let o = state.pilot_observation(seat, brain.site_request());
            let action = brain.intent(&o);
            assert!(action.horizontal.is_finite() && action.horizontal.abs() <= 1.0);
            assert!(!(action.primary_held && action.interact_held && action.brake_held));
            if !o.controls_armed {
                assert_eq!(action, SurfaceSortieAction::default());
            }
            if o.landing.phase == LandingPhase::Landed {
                saw_landing = true;
            }
            if o.location == PilotLocation::OnFoot
                && o.planet
                    .claim
                    .as_ref()
                    .is_some_and(|c| c.owner == Some(owner))
            {
                assert!(saw_landing);
                assert_eq!(o.supported_planet, Some(0));
                saw_claim = true;
            }
            if brain.telemetry().completed_tick.is_some() {
                assert!(saw_claim);
                assert!(matches!(o.location, PilotLocation::Aboard(_)));
                completion.get_or_insert(tick);
                // Remain finite and airborne after departure; a completed
                // milestone must not hide an immediate crash back to ground.
                assert!(
                    o.ship.position.distance_to(o.planet.motion.position) > o.planet.radius + 20.0
                );
                if tick - completion.unwrap() > 300 {
                    break;
                }
            }
            SurfaceSortieScenario::step(&mut state, &[action.encode(owner)], DT);
        }
        assert!(
            completion.is_some(),
            "case {case} seat {seat}: {:?}\n{:?}",
            brain.telemetry(),
            state.observation(seat)
        );
        assert!(state.terrain_diagnostics().issues.is_empty());
    }
}

#[test]
fn flight_north_captures_and_departs_in_either_seat() {
    sortie(0);
}
#[test]
fn flight_south_captures_and_departs_in_either_seat() {
    sortie(1);
}
#[test]
fn flight_east_retries_blocked_hatch_and_departs_in_either_seat() {
    sortie(2);
}
#[test]
fn flight_diagonal_captures_and_departs_in_either_seat() {
    sortie(3);
}

#[test]
fn cloned_policy_and_world_replay_and_reset_without_advancing_on_repeated_observations() {
    let reset = BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: 42,
    };
    let mut a =
        SurfaceSortieScenario::init_material_flight(42, 1, &[(reset.actor, cases::STARTS[0])]);
    let mut brain = RulePilotV1::new(reset);
    for _ in 0..120 {
        let o = a.pilot_observation(0, brain.site_request());
        let action = brain.intent(&o);
        let before = brain.telemetry().clone();
        assert_eq!(brain.intent(&o), action);
        assert_eq!(brain.telemetry(), &before);
        SurfaceSortieScenario::step(&mut a, &[action.encode(reset.actor)], DT);
    }
    let mut b = a.clone();
    let mut other = brain.clone();
    for _ in 0..180 {
        let x = a.pilot_observation(0, brain.site_request());
        let y = b.pilot_observation(0, other.site_request());
        assert_eq!(x, y);
        let action = brain.intent(&x);
        assert_eq!(action, other.intent(&y));
        SurfaceSortieScenario::step(&mut a, &[action.encode(reset.actor)], DT);
        SurfaceSortieScenario::step(&mut b, &[action.encode(reset.actor)], DT);
    }
    brain.reset(reset);
    assert_eq!(brain.telemetry(), RulePilotV1::new(reset).telemetry());
    assert_eq!(brain.site_request(), None);
}

#[test]
fn dirty_queries_preserve_a_site_but_completed_removal_invalidates_it() {
    let mut state = SurfaceSortieScenario::init_material_flight(
        42,
        1,
        &[(PlayerId::PLAYER_1, cases::STARTS[0])],
    );
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut brain = RulePilotV1::new(BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: 42,
    });
    let mut o = state.pilot_observation(0, None);
    brain.intent(&o);
    let site = brain.site_request();
    assert!(site.is_some());
    o.tick += 1;
    o.queries_ready = false;
    o.sites.clear();
    brain.intent(&o);
    assert_eq!(brain.site_request(), site);
    assert_eq!(brain.telemetry().invalidations, 0);
    o.tick += 1;
    o.queries_ready = true;
    brain.intent(&o);
    assert_eq!(brain.site_request(), None);
    assert_eq!(brain.telemetry().invalidations, 1);
    assert_eq!(brain.telemetry().goal, PilotGoal::Survey);
}

#[test]
fn wrong_actor_is_neutral_even_when_a_host_reuses_a_tick() {
    let mut state = SurfaceSortieScenario::init_material_flight(
        42,
        1,
        &[(PlayerId::PLAYER_1, cases::STARTS[0])],
    );
    SurfaceSortieScenario::step(&mut state, &[], DT);
    let mut brain = RulePilotV1::new(BrainReset {
        actor: PlayerId::PLAYER_1,
        episode_seed: 42,
    });
    let mut o = state.pilot_observation(0, None);
    brain.intent(&o);
    o.owner = PlayerId::PLAYER_2;
    assert_eq!(brain.intent(&o), SurfaceSortieAction::default());
    assert_eq!(brain.telemetry().goal, PilotGoal::Blocked);
}
