//! Controller contracts; physical contested approaches run in surface_flag_soak.
use engine_common::{CombatBreakSettings, Scenario};
use scenario_spacewars::{
    PlayerId,
    surface_sortie::{
        LandingPhase, PlanetFlagObservation, SurfaceSortieScenario, TransferResult,
        combat::TacticalSortieObservationV1,
        ground_navigation::{GroundRouteDiagnostics, GroundRouteFailure},
        landing_objective::{LandingObjective, LandingObjectiveRoute, LandingObjectiveSurvey},
    },
};
use spacewars_ai::{BrainReset, tactical_capture::TacticalCapturePilot};
use std::time::Duration;

fn diagnostics(length: f32) -> GroundRouteDiagnostics {
    GroundRouteDiagnostics {
        failure: None,
        partial: false,
        start_node: Some(0),
        start_distance: Some(0.0),
        destination_nodes: 1,
        nearest_destination_distance: Some(0.9),
        reachable_nodes: 4,
        closest_reachable_distance: Some(0.9),
        length,
        jumps: 0,
        flights: 0,
    }
}

fn fixture() -> (TacticalCapturePilot, TacticalSortieObservationV1) {
    let mut state = SurfaceSortieScenario::init_material_combat(42);
    SurfaceSortieScenario::step(&mut state, &[], Duration::from_nanos(16_666_667));
    let mut o = state.tactical_sortie_observation(0, None);
    o.sun = None;
    o.combat.target = None;
    let p = &mut o.combat.recovery.flight.pilot;
    p.controls_armed = true;
    p.sites.truncate(2);
    assert_eq!(p.sites.len(), 2);
    p.ship.position = p.sites[0].vehicle_position + p.sites[0].normal * 70.0;
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = Some(PlayerId::PLAYER_2);
    claim.flag = Some(PlanetFlagObservation {
        player: PlayerId::PLAYER_2,
        position: p.sites[1].hatch_position,
        normal: p.sites[1].normal,
        raised_fraction: 1.0,
    });
    let mut sites: Vec<_> = p
        .sites
        .iter()
        .map(|site| LandingObjectiveRoute {
            site: Some(site.id),
            outbound: diagnostics(4.0),
            returning: Some(diagnostics(4.0)),
        })
        .collect();
    sites[0].returning.as_mut().unwrap().failure = Some(GroundRouteFailure::Disconnected);
    o.landing_objective = Some(LandingObjectiveSurvey {
        version: 1,
        actor: p.owner,
        tick: p.tick,
        objective: LandingObjective::read(p).unwrap(),
        sites,
        actual: None,
    });
    (
        TacticalCapturePilot::new(
            BrainReset {
                actor: p.owner,
                episode_seed: 42,
            },
            CombatBreakSettings::default(),
        ),
        o,
    )
}

#[test]
fn landing_requires_both_directions_and_scores_the_walk_as_well_as_the_flight() {
    for missing_return in [false, true] {
        let (mut pilot, mut o) = fixture();
        if !missing_return {
            let route = &mut o.landing_objective.as_mut().unwrap().sites[0];
            route.outbound = diagnostics(100.0);
            route.returning = Some(diagnostics(100.0));
        }
        let before = o.clone();
        let action = pilot.intent(&o);
        assert_eq!(
            pilot.site_request(),
            Some(o.combat.recovery.flight.pilot.sites[1].id)
        );
        assert!(!action.flight.controls.interact_held);
        assert_eq!(o, before);
        let telemetry = pilot.telemetry().clone();
        assert_eq!(pilot.intent(&o), action);
        assert_eq!(pilot.telemetry(), &telemetry);
        let mut copy = pilot.clone();
        o.combat.recovery.flight.pilot.tick += 1;
        o.landing_objective = None;
        assert_eq!(pilot.intent(&o), copy.intent(&o));
    }
}

#[test]
fn stale_missing_partial_or_malformed_route_evidence_cannot_choose_a_site() {
    for fault in 0..9 {
        let (mut pilot, mut o) = fixture();
        let s = o.landing_objective.as_mut().unwrap();
        match fault {
            0 => s.version += 1,
            1 => s.actor = PlayerId::PLAYER_2,
            2 => s.tick += 1,
            3 => s.objective.revision += 1,
            4 => s.objective.position.x += 3.0,
            5 => s.sites[1].outbound.partial = true,
            6 => s.sites[1].outbound.length = f32::NAN,
            7 => s.sites[1].returning = None,
            _ => o.landing_objective = None,
        }
        assert!(!pilot.intent(&o).flight.controls.interact_held);
        assert_eq!(pilot.site_request(), None, "fault {fault}");
    }
}

#[test]
fn touchdown_checks_actual_access_and_changed_ownership_discards_the_old_plan() {
    let (mut pilot, mut o) = fixture();
    pilot.intent(&o);
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick += 1;
    p.landing.phase = LandingPhase::Landed;
    p.transfer = TransferResult::Ready;
    o.landing_objective = None;
    assert!(
        !pilot.intent(&o).flight.controls.interact_held,
        "wait for actual route measurement"
    );
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick += 1;
    o.landing_objective = Some(LandingObjectiveSurvey {
        version: 1,
        actor: p.owner,
        tick: p.tick,
        objective: LandingObjective::read(p).unwrap(),
        sites: vec![],
        actual: Some(LandingObjectiveRoute {
            site: None,
            outbound: diagnostics(1.0),
            returning: None,
        }),
    });
    assert!(!pilot.intent(&o).flight.controls.interact_held);
    assert_eq!(pilot.telemetry().objective_replans, 1);
    assert_eq!(pilot.site_request(), None);

    let (mut pilot, mut o) = fixture();
    pilot.intent(&o);
    let p = &mut o.combat.recovery.flight.pilot;
    p.tick += 1;
    p.planet.claim.as_mut().unwrap().flag = None;
    p.planet.claim.as_mut().unwrap().owner = None;
    o.landing_objective = None;
    pilot.intent(&o);
    assert_eq!(pilot.telemetry().objective_replans, 1);
    assert_eq!(pilot.site_request(), None);
    assert!(pilot.telemetry().landing.claimed_tick.is_none());
}

#[test]
fn terrain_edits_revalidate_access_without_abandoning_an_unchanged_route() {
    for blocked in [false, true] {
        let (mut pilot, mut o) = fixture();
        pilot.intent(&o);
        let selected = pilot.site_request();
        let p = &mut o.combat.recovery.flight.pilot;
        p.tick += 1;
        p.planet.revision += 1;
        for site in &mut p.sites {
            site.revision = p.planet.revision;
        }
        let survey = o.landing_objective.as_mut().unwrap();
        survey.tick = p.tick;
        survey.objective.revision = p.planet.revision;
        if blocked {
            survey.sites[1].returning = None;
        }
        pilot.intent(&o);
        assert_eq!(pilot.site_request(), if blocked { None } else { selected });
        assert_eq!(pilot.telemetry().objective_replans, u32::from(blocked));
        assert!(pilot.telemetry().landing.claimed_tick.is_none());
    }
}

#[test]
fn a_short_walk_does_not_override_shelter_from_a_nearby_opponent() {
    use scenario_spacewars::surface_sortie::combat::LandingCover;
    let (mut pilot, mut o) = fixture();
    let mut scene = SurfaceSortieScenario::init_material_combat(42);
    SurfaceSortieScenario::step(&mut scene, &[], Duration::from_nanos(16_666_667));
    let mut enemy = scene
        .tactical_sortie_observation(0, None)
        .combat
        .target
        .unwrap();
    let p = &o.combat.recovery.flight.pilot;
    enemy.motion.position = p.ship.position + engine_core::Vec2::Y * 100.0;
    enemy.ground_occluded = false;
    o.combat.target = Some(enemy);
    o.cover = p
        .sites
        .iter()
        .enumerate()
        .map(|(i, site)| LandingCover {
            site: site.id,
            grounded: i == 1,
            approach: i == 1,
            departure: i == 1,
        })
        .collect();
    let survey = o.landing_objective.as_mut().unwrap();
    survey.sites[0].outbound = diagnostics(1.0);
    survey.sites[0].returning = Some(diagnostics(1.0));
    survey.sites[1].outbound = diagnostics(60.0);
    survey.sites[1].returning = Some(diagnostics(60.0));
    pilot.intent(&o);
    assert_eq!(pilot.site_request(), Some(p.sites[1].id));
}
