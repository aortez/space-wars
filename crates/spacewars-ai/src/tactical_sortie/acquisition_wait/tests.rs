use super::*;
use crate::tactical_sortie::tests::{context, observation};
use scenario_spacewars::surface_sortie::{
    LandingPhase, PlanetFlagObservation, SolarHazard, live_planning::ObjectiveWorkState,
    pilot::LandingSiteQuery,
};

fn waiting() -> (TacticalSortiePilot, TacticalSortieObservationV1) {
    let mut o = observation();
    o.sun = None;
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick = 100;
    p.controls_armed = true;
    p.location = PilotLocation::Aboard(p.vehicle);
    p.landing.phase = LandingPhase::Flying;
    p.landing.supported_feet = 0;
    p.landing.foot_clearances = [26.0; 2];
    p.ship.position = p.planet.motion.position + Vec2::Y * (p.planet.radius + 80.0);
    p.ship.velocity = p.planet.velocity_at(p.ship.position);
    p.sites.clear();
    p.site_query = LandingSiteQuery::Deferred { next_tick: 115 };
    let mut pilot = TacticalSortiePilot::with_committed_descent(context(), Default::default());
    pilot.enable_bounded_acquisition(true);
    (pilot, o)
}

#[test]
fn deferred_or_missing_measurements_expire_as_a_budget_not_an_unreachable_verdict() {
    let (mut pilot, mut o) = waiting();
    pilot.intent(&o);
    let deadline = pilot.telemetry.acquisition_wait.unwrap().deadline_tick;
    o.combat.recovery.flight.pilot.tick = deadline - 1;
    pilot.intent(&o);
    assert!(pilot.telemetry.failed_tick.is_none());
    let mut copy = pilot.clone();
    o.combat.recovery.flight.pilot.tick = deadline;
    o.combat.recovery.flight.pilot.queries_ready = false;
    let intent = pilot.intent(&o);
    assert_eq!(copy.intent(&o), intent);
    assert_eq!(copy.telemetry(), pilot.telemetry());
    assert_eq!(pilot.intent(&o), intent);
    assert_eq!(pilot.telemetry.failed_tick, Some(deadline));
    assert_eq!(
        pilot.telemetry.acquisition_wait.unwrap().outcome,
        Some("deadline")
    );
    assert_eq!(pilot.telemetry.replans, 0);
    assert!(pilot.rejected_sites.is_empty());
    pilot.reset(context());
    assert!(pilot.bounded_acquisition);
    assert!(pilot.telemetry.acquisition_wait.is_none());
}

#[test]
fn repeated_invalidations_do_not_restart_the_acquisition_clock() {
    let (mut pilot, mut o) = waiting();
    let p = &mut o.combat.recovery.flight.pilot;
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(scenario_spacewars::PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: scenario_spacewars::PlayerId::PLAYER_2,
        position: p.planet.motion.position + Vec2::Y * p.planet.radius,
        normal: Vec2::Y,
        raised_fraction: 1.0,
    });
    o.objective_work = Some(ObjectiveWorkState::Stale);
    pilot.intent(&o);
    let initial = pilot.telemetry.acquisition_wait.unwrap();
    for tick in [200, 800, initial.deadline_tick - 1] {
        o.combat.recovery.flight.pilot.tick = tick;
        pilot.intent(&o);
        let wait = pilot.telemetry.acquisition_wait.unwrap();
        assert_eq!(wait.started_tick, initial.started_tick);
        assert_eq!(wait.deadline_tick, initial.deadline_tick);
        assert!(pilot.telemetry.failed_tick.is_none());
    }
    assert_eq!(pilot.telemetry.live_invalidations, 4);
    o.combat.recovery.flight.pilot.tick = initial.deadline_tick;
    pilot.intent(&o);
    assert_eq!(
        pilot.telemetry.acquisition.unwrap().reason,
        "acquisition_deadline"
    );
}

#[test]
fn first_choice_finishes_acquisition_and_later_landing_retries_do_not_reopen_it() {
    let (mut pilot, mut o) = waiting();
    pilot.intent(&o);
    let source = observation();
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick += 1;
    p.sites = source.combat.recovery.flight.pilot.sites;
    p.site_query = LandingSiteQuery::Survey;
    pilot.intent(&o);
    assert!(pilot.site.is_some());
    let wait = pilot.telemetry.acquisition_wait.unwrap();
    assert_eq!(wait.outcome, Some("selected"));
    pilot.replan(pilot.previous_tick.unwrap());
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick = wait.deadline_tick + 1;
    p.sites.clear();
    p.site_query = LandingSiteQuery::Deferred {
        next_tick: p.tick + 15,
    };
    pilot.intent(&o);
    assert!(pilot.telemetry.failed_tick.is_none());
    assert_eq!(
        pilot.telemetry.acquisition_wait.unwrap().finished_tick,
        wait.finished_tick
    );
}

#[test]
fn waiting_guidance_preserves_near_ground_and_solar_clearance_commands() {
    let (mut pilot, mut o) = waiting();
    pilot.intent(&o);
    assert_eq!(
        pilot.telemetry.acquisition_wait.unwrap().guidance,
        Some("initial_clearance")
    );
    o.combat.recovery.flight.pilot.tick += CLEARANCE_GRACE_TICKS;
    let action = pilot.intent(&o);
    assert!(!action.flight.controls.interact_held);
    assert_eq!(
        pilot.telemetry.acquisition_wait.unwrap().guidance,
        Some("hold_altitude")
    );
    o.combat.recovery.flight.pilot.tick += 1;
    o.combat.recovery.flight.pilot.landing.foot_clearances[0] = 3.0;
    pilot.intent(&o);
    assert_eq!(
        pilot.telemetry.acquisition_wait.unwrap().guidance,
        Some("ground_clearance")
    );
    o.combat.recovery.flight.pilot.tick += 1;
    o.combat.recovery.flight.pilot.landing.foot_clearances = [26.0; 2];
    o.sun = Some(SolarHazard {
        position: o.combat.recovery.flight.pilot.ship.position + Vec2::Y,
        radius: 200.0,
        heat_radius: 224.0,
    });
    pilot.intent(&o);
    assert_eq!(
        pilot.telemetry.acquisition_wait.unwrap().guidance,
        Some("solar_clearance")
    );
}
