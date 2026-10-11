use super::tests::{finish, fixture};
use super::*;
use crate::mission_policy::MissionPolicy;
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

fn current_neutral() -> (MissionEvaluator, MissionObservationV1, MissionTelemetry) {
    let (_, mut o, mut mission) = requested();
    mission.policy = "material_mission_v13";
    let mut host = MissionEvaluator::new(2)
        .with_flag_costs([true, false])
        .with_current_neutral_surveys([true, false]);
    let request = host.alternative_request(&o, &mission).unwrap();
    let template = o.destination_cover.as_ref().unwrap().candidates[0].clone();
    let candidates = request
        .candidates
        .into_iter()
        .flatten()
        .map(|id| {
            let mut candidate = template.clone();
            candidate.id = id;
            let planet = o.planets.iter().find(|p| p.index == id.planet).unwrap();
            let m = candidate.measurement.as_mut().unwrap();
            m.tick = request.generation;
            m.planet = planet.motion;
            m.revision = planet.revision;
            let site = m.site.as_mut().unwrap();
            site.id = id;
            site.revision = planet.revision;
            candidate
        })
        .collect();
    o.destination_cover = Some(DestinationCoverObservation {
        generation: request.generation,
        candidates,
    });
    o.local.combat.recovery.flight.pilot.tick += 1;
    (host, o, mission)
}

#[test]
fn current_neutral_requests_are_opt_in_per_seat_and_v13_only() {
    let (mut host, mut o, mut mission) = current_neutral();
    let request = host.alternative_request(&o, &mission).unwrap();
    let ids: Vec<_> = request.candidates.into_iter().flatten().collect();
    assert_eq!(ids.len(), 4);
    assert_eq!(Some(ids[0].planet), mission.target);
    assert_eq!(ids[0].planet, ids[1].planet);
    assert_ne!(ids[0], ids[1]);
    assert_ne!(ids[0].planet, ids[2].planet);
    assert_eq!(ids[2].planet, ids[3].planet);
    assert_ne!(ids[2], ids[3]);
    for policy in ["material_mission_v10", "material_mission_v12"] {
        mission.policy = policy;
        let old = host.alternative_request(&o, &mission).unwrap();
        assert_eq!(old.candidates.into_iter().flatten().count(), 2);
        assert!(
            old.candidates
                .into_iter()
                .flatten()
                .all(|id| Some(id.planet) != mission.target)
        );
    }
    mission.policy = "material_mission_v13";
    o.local.combat.recovery.flight.pilot.owner = PlayerId::PLAYER_2;
    let other = host.alternative_request(&o, &mission).unwrap();
    assert_eq!(other.candidates.into_iter().flatten().count(), 2);
    host.reset();
    assert!(host.surveys_current_neutral(PlayerId::PLAYER_1));
    assert!(!host.surveys_current_neutral(PlayerId::PLAYER_2));
}

#[test]
fn both_neutral_costs_are_available_before_a_native_site_choice() {
    let (mut host, o, mission) = current_neutral();
    assert!(o.local.combat.recovery.flight.pilot.sites.is_empty());
    assert!(mission.capture.is_none());
    host.observe(&o, &mission);
    let evidence = &host.actors[&0].evidence;
    assert_eq!(evidence.len(), 2);
    assert!(evidence.iter().all(|e| e.remote && e.costs.is_some()));
    assert!(
        evidence
            .iter()
            .any(|e| Some(e.key.planet) == mission.target)
    );
    let tick = o.local.combat.recovery.flight.pilot.tick;
    for dispatch in tick..tick + 4 {
        host.advance(dispatch, DEFAULT_WORK);
    }
    let result = host.latest(PlayerId::PLAYER_1).unwrap();
    assert_eq!(result.model, CURRENT_NEUTRAL_MODEL);
    let current = result.candidates.iter().find(|c| c.current).unwrap();
    assert!(current.total_seconds.is_some());
    assert!(current.evidence_tick.is_some_and(|source| source < tick));
    assert!(current.evidence_kind.contains("live feasibility unknown"));
}

#[test]
fn multi_planet_sources_are_independent_and_failures_revoke_only_their_cost() {
    let (host, valid, mission) = current_neutral();
    for mutation in 0..7 {
        let mut host = host.clone();
        host.observe(&valid, &mission);
        let mut o = valid.clone();
        o.local.combat.recovery.flight.pilot.tick += 1;
        let current = mission.target.unwrap();
        match mutation {
            0 => {
                for c in o
                    .destination_cover
                    .as_mut()
                    .unwrap()
                    .candidates
                    .iter_mut()
                    .filter(|c| c.id.planet == current)
                {
                    c.measurement.as_mut().unwrap().climb_clear = Some(false);
                }
            }
            1 => {
                o.planets
                    .iter_mut()
                    .find(|p| p.index == current)
                    .unwrap()
                    .revision += 1
            }
            2 => {
                o.planets
                    .iter_mut()
                    .find(|p| p.index == current)
                    .unwrap()
                    .claim
                    .as_mut()
                    .unwrap()
                    .owner = Some(PlayerId::PLAYER_2)
            }
            3 => o.local.combat.recovery.flight.pilot.tick += MAX_EVIDENCE_AGE,
            4 => o.local.combat.recovery.flight.pilot.queries_ready = false,
            5 => {
                // A refreshed failure on both bearings supersedes the old cost.
                for c in o
                    .destination_cover
                    .as_mut()
                    .unwrap()
                    .candidates
                    .iter_mut()
                    .filter(|c| c.id.planet == current)
                {
                    let m = c.measurement.as_mut().unwrap();
                    m.tick += 1;
                    m.finding = CoverFinding::NoLanding;
                }
            }
            _ => {
                o.local.combat.recovery.flight.pilot.tick =
                    valid.local.combat.recovery.flight.pilot.tick - 1;
                o.destination_cover.as_mut().unwrap().generation += 1;
            }
        }
        host.observe(&o, &mission);
        let evidence = &host.actors[&0].evidence;
        assert!(
            evidence
                .iter()
                .all(|e| e.key.planet != current || e.costs.is_none()),
            "mutation {mutation}"
        );
        if matches!(mutation, 0 | 1 | 2 | 5) {
            assert!(
                evidence
                    .iter()
                    .any(|e| e.key.planet != current && e.costs.is_some()),
                "other planet retained, mutation {mutation}"
            );
        }
    }
}

#[test]
fn current_neutral_demand_stays_stable_and_yields_to_local_work() {
    let (mut host, mut o, mission) = current_neutral();
    let request = host.alternative_request(&o, &mission).unwrap();
    o.local.combat.recovery.flight.pilot.tick += 1;
    o.local.combat.recovery.flight.pilot.ship.position.x += 2.0;
    for planet in &mut o.planets {
        planet.motion.position.x += 10.0;
    }
    assert_eq!(host.alternative_request(&o, &mission), Some(request));
    o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Survey;
    assert!(host.alternative_request(&o, &mission).is_none());
    o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::NotRequested;
    assert_eq!(host.alternative_request(&o, &mission), Some(request));
    o.planets
        .iter_mut()
        .find(|p| Some(p.index) == mission.target)
        .unwrap()
        .revision += 1;
    let changed = host.alternative_request(&o, &mission).unwrap();
    assert_ne!(changed.generation, request.generation);
}

#[test]
fn owned_current_destination_keeps_the_original_alternative_request() {
    let (mut candidate, mut o, mission) = current_neutral();
    let mut predecessor = MissionEvaluator::new(2).with_flag_costs([true, false]);
    o.planets
        .iter_mut()
        .find(|p| Some(p.index) == mission.target)
        .unwrap()
        .claim
        .as_mut()
        .unwrap()
        .owner = Some(PlayerId::PLAYER_2);
    assert_eq!(
        candidate.alternative_request(&o, &mission),
        predecessor.alternative_request(&o, &mission)
    );
}

#[test]
fn v13_survey_options_leave_newer_policy_demand_and_models_unchanged() {
    for policy in [
        MissionPolicy::SurveyValuePlanner,
        MissionPolicy::LandingPlanPlanner,
        MissionPolicy::ApproachSurveyPlanner,
    ] {
        let (_, mut o, mut mission) = current_neutral();
        mission.policy = policy.id();
        let mut retained = MissionEvaluator::new(2);
        let mut configured = MissionEvaluator::new(2)
            .with_flag_costs([true, false])
            .with_current_neutral_surveys([true, false]);
        for tick in [100, 129, 130, 160] {
            let p = &mut o.local.combat.recovery.flight.pilot;
            p.tick = tick;
            p.ship.position = p.ship.position.rotate_radians(0.5);
            let request = retained.alternative_request(&o, &mission).unwrap();
            assert_eq!(configured.alternative_request(&o, &mission), Some(request));
            assert_eq!(request.candidates.into_iter().flatten().count(), 2);
            assert!(
                request
                    .candidates
                    .into_iter()
                    .flatten()
                    .all(|id| { Some(id.planet) == mission.target })
            );
            for evaluator in [&mut retained, &mut configured] {
                evaluator.observe(&o, &mission);
                for dispatch in tick..tick + 4 {
                    evaluator.advance(dispatch, DEFAULT_WORK);
                }
                assert_eq!(
                    evaluator.latest(PlayerId::PLAYER_1).unwrap().model,
                    model_for_policy(policy.id())
                );
            }
            assert_eq!(
                serde_json::to_value(configured.latest(PlayerId::PLAYER_1)).unwrap(),
                serde_json::to_value(retained.latest(PlayerId::PLAYER_1)).unwrap()
            );
        }
    }
}

fn approach_fixture(
    policy: MissionPolicy,
) -> (MissionEvaluator, MissionObservationV1, MissionTelemetry) {
    let (_, mut o, bot) = fixture();
    let p = &mut o.local.combat.recovery.flight.pilot;
    p.tick = 100;
    p.site_query = LandingSiteQuery::NotRequested;
    p.landing.supported_feet = 0;
    p.planet.motion.position = Vec2::ZERO;
    p.planet.motion.angle = std::f32::consts::FRAC_PI_2;
    p.ship.position = -Vec2::X * 200.0;
    let claim = p.planet.claim.as_mut().unwrap();
    claim.owner = None;
    claim.flag = None;
    o.planets = vec![p.planet.clone()];
    let mut mission = bot.telemetry().clone();
    mission.policy = policy.id();
    mission.target = Some(p.planet.index);
    o.local.combat.target = None;
    (MissionEvaluator::new(1), o, mission)
}

#[test]
fn approach_requests_refresh_at_most_each_thirty_ticks_and_yield_to_local_sensing() {
    for policy in [
        MissionPolicy::LandingPlanPlanner,
        MissionPolicy::ApproachSurveyPlanner,
    ] {
        let (mut e, mut o, m) = approach_fixture(policy);
        let first = e.alternative_request(&o, &m).unwrap();
        o.local.combat.recovery.flight.pilot.ship.position = Vec2::Y * 200.0;
        o.local.combat.recovery.flight.pilot.tick = 129;
        assert_eq!(e.alternative_request(&o, &m), Some(first));
        o.local.combat.recovery.flight.pilot.tick = 130;
        o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::Survey;
        assert!(e.alternative_request(&o, &m).is_none());
        o.local.combat.recovery.flight.pilot.site_query = LandingSiteQuery::NotRequested;
        let changed = e.alternative_request(&o, &m).unwrap();
        if policy == MissionPolicy::LandingPlanPlanner {
            assert_eq!(changed, first);
        } else {
            assert_eq!(changed.generation, 130);
            assert_ne!(changed.candidates, first.candidates);
            assert_eq!(changed.candidates.iter().flatten().count(), 2);
            o.local.combat.recovery.flight.pilot.tick = 1000;
            assert_eq!(
                e.alternative_request(&o, &m),
                Some(changed),
                "unchanged bearings retain generation"
            );
        }
    }
}

fn measured_approaches(
    policy: MissionPolicy,
) -> (MissionEvaluator, MissionObservationV1, MissionTelemetry) {
    let (mut e, mut o, m) = approach_fixture(policy);
    let request = e.alternative_request(&o, &m).unwrap();
    let planet = &o.planets[0];
    let mut measured_planet = planet.motion;
    measured_planet.position = Vec2::new(100.0, 100.0);
    measured_planet.angle = 0.0;
    let candidates = request
        .candidates
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, id)| {
            let local =
                Vec2::Y.rotate_radians(f32::from(id.bearing) * std::f32::consts::TAU / 64.0);
            let point = measured_planet.position + local * planet.radius;
            CoverCandidate {
                id,
                status: CoverStatus::Stale,
                reason: Some("historical measurement"),
                measurement: Some(CoverMeasurement {
                    tick: request.generation + i as u64,
                    revision: planet.revision,
                    planet: measured_planet,
                    ship_form: ShipForm::Ship,
                    opponent: None,
                    queries: 40,
                    finding: CoverFinding::Measured,
                    cover: None,
                    climb_clear: Some(true),
                    site: Some(PilotLandingSite {
                        id,
                        revision: planet.revision,
                        local_position: local * planet.radius,
                        position: point,
                        normal: local,
                        velocity: Vec2::ZERO,
                        vehicle_position: point + local * 5.45,
                        hatch_position: point,
                        boarding_hatches: [Some(point), None],
                        hatch_has_settling_margin: true,
                    }),
                }),
            }
        })
        .collect();
    o.destination_cover = Some(DestinationCoverObservation {
        generation: request.generation,
        candidates,
    });
    o.local.combat.recovery.flight.pilot.tick = 102;
    (e, o, m)
}

#[test]
fn approach_ranking_reprojects_material_geometry_and_preserves_original_measurement_age() {
    for policy in [
        MissionPolicy::LandingPlanPlanner,
        MissionPolicy::ApproachSurveyPlanner,
    ] {
        let (mut e, mut o, m) = measured_approaches(policy);
        e.observe(&o, &m);
        let s = &e.actors[&0].evidence[0];
        assert_eq!(
            s.site.bearing,
            if policy == MissionPolicy::ApproachSurveyPlanner {
                0
            } else {
                16
            }
        );
        assert_eq!(
            s.tick,
            if policy == MissionPolicy::ApproachSurveyPlanner {
                100
            } else {
                101
            }
        );
        assert_eq!(s.costs, Some(model::no_flag_costs()));
        o.destination_cover.as_mut().unwrap().candidates.reverse();
        let site = s.site;
        o.local.combat.recovery.flight.pilot.tick += 1;
        e.observe(&o, &m);
        assert_eq!(
            e.actors[&0].evidence[0].site, site,
            "ranking does not depend on publication order"
        );
    }
}

#[test]
fn approach_score_ties_prefer_recency_then_bearing() {
    for equal_age in [false, true] {
        let (mut e, mut o, m) = measured_approaches(MissionPolicy::ApproachSurveyPlanner);
        let samples = &mut o.destination_cover.as_mut().unwrap().candidates;
        // Equal vehicle points isolate exact score ties from trigonometric rounding.
        let first = samples[0].measurement.as_ref().unwrap().clone();
        let second = samples[1].measurement.as_mut().unwrap();
        second.site.as_mut().unwrap().vehicle_position = first.site.unwrap().vehicle_position;
        if equal_age {
            second.tick = first.tick;
        }
        samples.reverse();
        e.observe(&o, &m);
        assert_eq!(
            e.actors[&0].evidence[0].site.bearing,
            if equal_age { 0 } else { 16 }
        );
    }
}

#[test]
fn approach_ranking_rejects_degenerate_geometry_and_never_renews_expired_sources() {
    for mutation in 0..6 {
        let (mut e, mut o, m) = measured_approaches(MissionPolicy::ApproachSurveyPlanner);
        match mutation {
            0 => o.local.combat.recovery.flight.pilot.ship.position = o.planets[0].motion.position,
            1 => o.planets[0].motion.angle = f32::NAN,
            2 => {
                for c in &mut o.destination_cover.as_mut().unwrap().candidates {
                    c.measurement
                        .as_mut()
                        .unwrap()
                        .site
                        .as_mut()
                        .unwrap()
                        .vehicle_position
                        .x = f32::NAN;
                }
            }
            3 => o.local.combat.recovery.flight.pilot.tick += MAX_EVIDENCE_AGE,
            4 => o.local.combat.recovery.flight.pilot.ship.position.x = f32::MAX / 2.0,
            _ => {
                for c in &mut o.destination_cover.as_mut().unwrap().candidates {
                    c.measurement
                        .as_mut()
                        .unwrap()
                        .site
                        .as_mut()
                        .unwrap()
                        .vehicle_position
                        .x = f32::MAX / 2.0;
                }
            }
        }
        e.observe(&o, &m);
        assert!(
            e.actors[&0].evidence.iter().all(|s| s.costs.is_none()),
            "mutation {mutation}"
        );
    }
}
