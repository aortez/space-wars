use super::tests::{finish, fixture};
use super::*;
use scenario_spacewars::surface_sortie::{
    destination_cover::{
        CoverCandidate, CoverFinding, CoverMeasurement, CoverStatus, DestinationCoverObservation,
    },
    pilot::{LandingSiteQuery, PilotLandingSite},
};

fn requested() -> (MissionEvaluator, MissionObservationV1, MissionTelemetry) {
    let (_, mut o, bot) = fixture();
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.site_query = LandingSiteQuery::NotRequested;
    p.landing.supported_feet = 0;
    p.sites.clear();
    let mut mission = bot.telemetry().clone();
    mission.target = Some(p.planet.index);
    let mut host = MissionEvaluator::new(1);
    let request = host.alternative_request(&o, &mission).unwrap();
    let id = request.candidates[0].unwrap();
    let planet = o.planets.iter().find(|p| p.index == id.planet).unwrap();
    let site = PilotLandingSite {
        id,
        revision: planet.revision,
        local_position: Vec2::Y * planet.radius,
        position: planet.motion.position + Vec2::Y * planet.radius,
        normal: Vec2::Y,
        velocity: Vec2::ZERO,
        vehicle_position: planet.motion.position + Vec2::Y * (planet.radius + 5.45),
        hatch_position: planet.motion.position + Vec2::Y * planet.radius,
        boarding_hatches: [Some(planet.motion.position + Vec2::Y * planet.radius), None],
        hatch_has_settling_margin: true,
    };
    o.destination_cover = Some(DestinationCoverObservation {
        generation: request.generation,
        candidates: vec![CoverCandidate {
            id,
            status: CoverStatus::Stale,
            reason: Some("physics advanced"),
            measurement: Some(CoverMeasurement {
                tick: request.generation,
                revision: planet.revision,
                planet: planet.motion,
                ship_form: ShipForm::Ship,
                opponent: None,
                queries: 40,
                finding: CoverFinding::Measured,
                site: Some(site),
                cover: None,
                climb_clear: Some(true),
            }),
        }],
    });
    o.local.combat.recovery.flight.pilot.tick += 1;
    (host, o, mission)
}

#[test]
fn alternative_requests_are_bounded_stable_and_yield_to_local_work() {
    let (mut host, mut o, mission) = requested();
    let request = host.alternative_request(&o, &mission).unwrap();
    let ids: Vec<_> = request.candidates.into_iter().flatten().collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0].planet, ids[1].planet);
    assert_ne!(Some(ids[0].planet), mission.target);
    assert_ne!(ids[0], ids[1]);
    assert!(request.sample_climb);
    assert_eq!(
        host.clone().alternative_request(&o, &mission),
        Some(request)
    );
    o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Survey;
    assert!(host.alternative_request(&o, &mission).is_none());
    o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::NotRequested;
    assert_eq!(host.alternative_request(&o, &mission), Some(request));
    o.local.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_2;
    assert!(
        host.alternative_request(&o, &mission).is_none(),
        "bounded actor capacity"
    );
    o.local.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_1;
    o.match_context.as_mut().unwrap().finished = true;
    assert!(host.alternative_request(&o, &mission).is_none());
    host.reset();
    assert!(host.actors.is_empty());
}

#[test]
fn remote_costs_keep_source_age_and_never_claim_live_or_combat_feasibility() {
    let (mut host, o, mission) = requested();
    host.observe(&o, &mission);
    let evidence = &host.actors[&0].evidence;
    assert_eq!(evidence.len(), 1);
    assert!(evidence[0].remote);
    let result = finish(snapshot(&o, &mission, evidence));
    let remote = result
        .candidates
        .iter()
        .find(|c| c.planet == evidence[0].key.planet)
        .unwrap();
    assert!(remote.total_seconds.is_some());
    assert_eq!(remote.evidence_tick, Some(1));
    assert_eq!(remote.evidence_age_ticks, Some(1));
    assert!(remote.evidence_kind.contains("live feasibility unknown"));
    assert!(result.combat_risk.contains("unmodelled"));
    assert!(
        result.preferred_by_time.is_none(),
        "remaining unknowns prevent a complete ranking"
    );
    assert!(remote.route_source_tick.is_none());
}

#[test]
fn incompatible_remote_samples_cannot_produce_costs() {
    let (host, valid, mission) = requested();
    for mutation in 0..11 {
        let mut host = host.clone();
        let mut o = valid.clone();
        let r = o.destination_cover.as_mut().unwrap();
        let c = &mut r.candidates[0];
        let m = c.measurement.as_mut().unwrap();
        match mutation {
            0 => r.generation += 1,
            1 => m.tick += 100,
            2 => m.revision += 1,
            3 => m.finding = CoverFinding::Incomplete,
            4 => m.finding = CoverFinding::NoLanding,
            5 => m.site.as_mut().unwrap().boarding_hatches = [None; 2],
            6 => m.climb_clear = None,
            7 => m.climb_clear = Some(false),
            8 => m.site.as_mut().unwrap().id.bearing += 1,
            9 => o.local.combat.recovery.flight.pilot.queries_ready = false,
            _ => {
                o.planets
                    .iter_mut()
                    .find(|p| p.index == c.id.planet)
                    .unwrap()
                    .claim
                    .as_mut()
                    .unwrap()
                    .owner = Some(PlayerId::PLAYER_2)
            }
        }
        host.observe(&o, &mission);
        assert!(
            host.actors[&0].evidence.iter().all(|e| e.costs.is_none()),
            "mutation {mutation}"
        );
    }
}

#[test]
fn retained_alternative_is_revoked_by_new_failure_edit_ownership_and_expiry() {
    let (mut host, valid, mission) = requested();
    host.observe(&valid, &mission);
    let planet = host.actors[&0].evidence[0].key.planet;
    for mutation in 0..5 {
        let mut host = host.clone();
        let mut o = valid.clone();
        o.local.combat.recovery.flight.pilot.tick += 1;
        match mutation {
            0 => {
                o.destination_cover.as_mut().unwrap().candidates[0]
                    .measurement
                    .as_mut()
                    .unwrap()
                    .climb_clear = Some(false);
            }
            1 => {
                o.planets
                    .iter_mut()
                    .find(|p| p.index == planet)
                    .unwrap()
                    .revision += 1;
            }
            2 => {
                o.planets
                    .iter_mut()
                    .find(|p| p.index == planet)
                    .unwrap()
                    .claim
                    .as_mut()
                    .unwrap()
                    .owner = Some(PlayerId::PLAYER_2);
            }
            3 => {
                o.local.combat.recovery.flight.pilot.tick += MAX_EVIDENCE_AGE;
            }
            _ => {
                o.local.combat.recovery.flight.pilot.queries_ready = false;
            }
        }
        host.observe(&o, &mission);
        assert!(
            host.actors[&0].evidence.iter().all(|e| e.costs.is_none()),
            "mutation {mutation}"
        );
    }
}
